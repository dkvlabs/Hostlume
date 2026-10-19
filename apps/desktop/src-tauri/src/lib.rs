//! APP_NAME desktop shell — Tauri 2 entry point.
//!
//! Security posture (security-privacy.md): strict CSP, `core:default`
//! capability only, no shell/filesystem plugins, all command inputs
//! validated in Rust, no remote listener, no privileged helper.

pub mod collectors;
pub mod commands;
pub mod processes;
mod scheduler;
mod settings;
mod settings_store;
mod state;

use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use monitor_core::{CapabilityReport, Clock, Collector};
use tauri::Manager;

use crate::state::AppState;

/// Aggregate the capability report from the platform adapters. Rows the
/// current host cannot own (cross-compiled `WRONG_PLATFORM` skeletons) are
/// dropped; what remains is this build's honest capability set.
fn build_capability_report(host_id: &str, generated_at_utc: String) -> CapabilityReport {
    let mut collectors: Vec<Box<dyn Collector>> = vec![
        Box::new(monitor_platform_macos::PlatformCollector::new()),
        Box::new(monitor_platform_windows::PlatformCollector::new()),
        Box::new(monitor_platform_linux::PlatformCollector::new()),
    ];
    let mut report = CapabilityReport::new(host_id, generated_at_utc);
    for collector in &mut collectors {
        for capability in collector.capabilities() {
            if capability.reason_code.as_deref() != Some("WRONG_PLATFORM") {
                report.capabilities.push(capability);
            }
        }
    }
    report
}

/// Read the retention horizons from settings with defaults on any error.
fn read_retention_horizons(app: &tauri::AppHandle) -> (u32, u32, u32) {
    let defaults = (
        monitor_history::DEFAULT_RAW_HOURS,
        monitor_history::DEFAULT_ROLLUP_1M_DAYS,
        monitor_history::DEFAULT_ROLLUP_1H_DAYS,
    );
    let Some(state) = app.try_state::<AppState>() else {
        return defaults;
    };
    match state.store.load_settings() {
        Ok(settings) => (
            settings.history.raw_hours,
            settings.history.rollup_1m_days,
            settings.history.rollup_1h_days,
        ),
        Err(_) => defaults,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            let db_path = data_dir.join("settings.sqlite3");
            let store = Arc::new(settings_store::SettingsStore::open(&db_path)?);
            let host_id = store.host_id()?;
            let settings = store.load_settings()?;

            let generation = uuid::Uuid::new_v4().to_string();
            let capabilities =
                build_capability_report(&host_id, monitor_core::SystemClock.now_utc_iso8601());
            let capabilities_revision = Arc::new(AtomicU64::new(capabilities.revision));
            let latest: Arc<RwLock<Option<monitor_core::SystemSnapshot>>> = Arc::default();
            let polling_secs = Arc::new(AtomicU32::new(settings.polling_base_seconds));
            let diagnostics: scheduler::DiagnosticsMap = Arc::default();

            // History: bounded writer owns the write connection; read-side
            // queries open their own connection to the same WAL database.
            let history_dir = data_dir.join("history");
            std::fs::create_dir_all(&history_dir)?;
            let history_db = rusqlite::Connection::open(history_dir.join("history.sqlite3"))?;
            let history_writer = Arc::new(
                monitor_history::BatchWriter::spawn(history_db).map_err(|err| {
                    std::io::Error::other(format!("history migration failed: {err}"))
                })?,
            );

            // History tap: every published snapshot's available numeric
            // readings are enqueued for persistence. Drops under disk
            // pressure are counted by the writer — never blocking collection.
            let tap_writer = Arc::clone(&history_writer);
            let snapshot_tap: scheduler::SnapshotTap =
                Some(Arc::new(move |snapshot: &monitor_core::SystemSnapshot| {
                    for reading in &snapshot.metrics {
                        let Some(monitor_core::MetricValue::Number(value)) = reading.value else {
                            continue;
                        };
                        tap_writer.enqueue(monitor_history::WriteItem::Sample {
                            host_id: snapshot.host_id.clone(),
                            metric_id: reading.metric_id.clone(),
                            entity_id: reading.entity_id.clone(),
                            ts_utc_ms: monitor_core::SystemClock.now_epoch_ms(),
                            value_real: value,
                            source: reading.source.clone(),
                            generation: snapshot.generation.clone(),
                            sequence: reading.monotonic_sequence,
                        });
                    }
                }));

            let emit_handle = handle.clone();
            let emitter: scheduler::TelemetryEmitter = Arc::new(move |payload| {
                use tauri::Emitter;
                let _ = emit_handle.emit(scheduler::TELEMETRY_EVENT, payload);
            });
            let scheduler_handle = scheduler::spawn(
                host_id.clone(),
                generation.clone(),
                latest.clone(),
                polling_secs.clone(),
                capabilities_revision.clone(),
                diagnostics.clone(),
                emitter,
                snapshot_tap,
            );

            // Maintenance: 1-minute rollups every pass (last 5 min recompute,
            // idempotent), hourly rollups + retention every 10th pass.
            let maintenance_handle = handle.clone();
            let maintenance_host_id = host_id.clone();
            tauri::async_runtime::spawn(async move {
                let mut pass: u32 = 0;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    pass = pass.wrapping_add(1);
                    let Ok(dir) = maintenance_handle.path().app_data_dir() else {
                        continue;
                    };
                    let Ok(mut conn) =
                        rusqlite::Connection::open(dir.join("history").join("history.sqlite3"))
                    else {
                        continue;
                    };
                    // Idempotent: the writer migrated at startup; a fresh DB
                    // here (e.g. writer failed to start) gets the schema too.
                    if monitor_history::apply_migrations(&mut conn, monitor_history::MIGRATIONS)
                        .is_err()
                    {
                        continue;
                    }
                    let now = monitor_core::SystemClock.now_epoch_ms();
                    let _ = monitor_history::compute_rollups(
                        &conn,
                        &maintenance_host_id,
                        monitor_history::RollupGranularity::OneMinute,
                        now - 5 * 60_000,
                        now,
                    );
                    if pass.is_multiple_of(10) {
                        let _ = monitor_history::compute_rollups(
                            &conn,
                            &maintenance_host_id,
                            monitor_history::RollupGranularity::OneHour,
                            now - 25 * 3_600_000,
                            now,
                        );
                        let (raw_h, r1m_d, r1h_d) = read_retention_horizons(&maintenance_handle);
                        let _ = monitor_history::retain(&conn, now, raw_h, r1m_d, r1h_d);
                    }
                }
            });

            app.manage(AppState {
                host_id: host_id.clone(),
                generation: generation.clone(),
                latest,
                capabilities,
                store,
                polling_secs,
                diagnostics,
                scheduler: scheduler_handle,
                history_writer: Mutex::new(Some(history_writer)),
                processes: Mutex::new(processes::ProcessManager::new()),
                history_dir: Mutex::new(Some(history_dir)),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Phase 01 policy: closing the main window exits the app. The
            // close-to-tray preference activates with the tray (Phase 06).
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.scheduler.stop();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_capabilities,
            commands::get_settings,
            commands::update_settings,
            commands::get_diagnostics,
            commands::get_process_page,
            commands::request_process_action,
            commands::get_history,
            commands::clear_history,
        ])
        .build(tauri::generate_context!())
        .expect("error while building APP_NAME")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.scheduler.stop();
                    commands::shutdown_history(&state);
                }
            }
        });
}
