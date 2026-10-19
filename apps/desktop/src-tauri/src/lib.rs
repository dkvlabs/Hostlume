//! APP_NAME desktop shell — Tauri 2 entry point.
//!
//! Security posture (security-privacy.md): strict CSP, `core:default`
//! capability only, no shell/filesystem plugins, all command inputs
//! validated in Rust, no remote listener, no privileged helper.

pub mod actions;
pub mod alerts;
pub mod collectors;
pub mod commands;
pub mod processes;
pub mod recordings;
pub mod remote;
mod scheduler;
mod settings;
mod settings_store;
mod state;
pub mod storage;
pub mod tray;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use monitor_core::{CapabilityReport, Clock, Collector};
use tauri::Manager;

use crate::state::AppState;

/// Aggregate the user-facing capability report (FX-021). Every collector
/// speaks for the families it owns — including `unsupported` rows with
/// reasons — and cross-compiled `WRONG_PLATFORM` rows are dropped. Each
/// metric family appears exactly once, from the source that probes it.
pub(crate) fn build_capability_report(host_id: &str, generated_at_utc: String) -> CapabilityReport {
    let mut collectors: Vec<Box<dyn Collector>> = vec![
        Box::new(collectors::CpuCollector::new()),
        Box::new(collectors::MemoryCollector::new()),
        Box::new(collectors::HostCollector::new()),
        Box::new(collectors::DisksCollector::new()),
        Box::new(collectors::NetworkCollector::new()),
        Box::new(collectors::DiskIoCollector::new()),
        Box::new(collectors::SensorsCollector::new()),
        Box::new(collectors::BatteryCollector::new()),
        Box::new(collectors::BluetoothCollector::new()),
        Box::new(collectors::GpuCollector::new()),
        // Platform adapters claim nothing natively; their cross-compile
        // rows are filtered below (compile guard, see ADR-0006).
        Box::new(monitor_platform_macos::PlatformCollector::new()),
        Box::new(monitor_platform_windows::PlatformCollector::new()),
        Box::new(monitor_platform_linux::PlatformCollector::new()),
    ];
    let mut report = CapabilityReport::new(host_id, generated_at_utc);
    let mut seen = std::collections::HashSet::new();
    for collector in &mut collectors {
        for capability in collector.capabilities() {
            if capability.reason_code.as_deref() != Some("WRONG_PLATFORM")
                && seen.insert(capability.metric_id.clone())
            {
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

/// WAL connection to the history database for the alert runtime (rules
/// CRUD + restart restore). The batch writer keeps its own connection;
/// WAL plus a busy timeout make both safe (ADR-0002).
fn open_history_connection(
    history_dir: &std::path::Path,
) -> Result<rusqlite::Connection, std::io::Error> {
    let mut conn = rusqlite::Connection::open(history_dir.join("history.sqlite3"))
        .map_err(std::io::Error::other)?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.busy_timeout(std::time::Duration::from_millis(2_000));
    monitor_history::apply_migrations(&mut conn, monitor_history::MIGRATIONS)
        .map_err(|err| std::io::Error::other(format!("alerts db migration failed: {err}")))?;
    Ok(conn)
}

/// One-line crash entry (phase 09 crash handling): UTC timestamp, thread,
/// panic location and a truncated payload. The payload is developer text,
/// not telemetry — the log stays in the app data directory and is never
/// uploaded anywhere.
fn crash_log_line(thread: Option<&str>, payload: &str, location: Option<&str>) -> String {
    const MAX_PAYLOAD: usize = 500;
    let truncated = if payload.chars().count() > MAX_PAYLOAD {
        let cut: String = payload.chars().take(MAX_PAYLOAD).collect();
        format!("{cut}…")
    } else {
        payload.to_string()
    };
    format!(
        "{} panic thread={:?} location={} payload={truncated:?}\n",
        monitor_core::SystemClock.now_utc_iso8601(),
        thread.unwrap_or("<unnamed>"),
        location.unwrap_or("<unknown>"),
    )
}

/// Append a prepared line, creating the file if needed. Best effort: a
/// panicking hook must never panic and must never mask the real failure.
fn append_crash_entry(path: &std::path::Path, line: &str) {
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new("/")));
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

/// Install the crash logger before any subsystem that can panic. The
/// default stderr hook is chained so console runs keep their output.
fn install_crash_logger(dir: &std::path::Path) {
    let path = dir.join("crash.log");
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_string()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "non-string panic payload".to_string()
        };
        let line = crash_log_line(
            std::thread::current().name(),
            &payload,
            info.location()
                .map(|location| location.to_string())
                .as_deref(),
        );
        append_crash_entry(&path, &line);
        previous(info);
    }));
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            // Crash handling first: every later subsystem can panic.
            install_crash_logger(&data_dir);
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

            // Tray controller (FX-015): one icon + native menu driven by
            // the snapshot bus below — never a second polling loop.
            let paused = Arc::new(AtomicBool::new(false));
            let rate_bits = settings.rate_unit == crate::settings::RateUnit::Bits;
            let tray_controller = match tray::TrayController::build(
                handle.clone(),
                settings.tray,
                rate_bits,
                paused.clone(),
                include_bytes!("../icons/icon.png"),
            ) {
                Ok(controller) => Some(Arc::new(controller)),
                Err(error) => {
                    eprintln!("tray unavailable: {error}");
                    None
                }
            };

            // Alert runtime (FX-017): persisted rules + restart-safe
            // cooldown restore; evaluated on the snapshot tap below.
            let alert_runtime = Arc::new(alerts::AlertRuntime::open(
                &open_history_connection(&history_dir)?,
                settings.alerts,
            )?);

            // Snapshot tap: alerts + history persistence + tray repaint,
            // one bus.
            let tap_writer = Arc::clone(&history_writer);
            let tap_tray = tray_controller.clone();
            let tap_app = handle.clone();
            let snapshot_tap: scheduler::SnapshotTap =
                Some(Arc::new(move |snapshot: &monitor_core::SystemSnapshot| {
                    {
                        // Alerts evaluate first so a fired rule's audit
                        // episode and this tick's samples share a clock.
                        if let Some(state) = tap_app.try_state::<AppState>() {
                            alerts::on_snapshot(&tap_app, &state, snapshot);
                        }
                    }
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
                    if let Some(tray) = tap_tray.as_ref() {
                        tray.update(snapshot);
                    }
                }));

            let emit_handle = handle.clone();
            // Emit failures were previously discarded; a webview that never
            // receives ticks shows stale data with no diagnostic. Report the
            // first failure and then one per 100 so the log stays bounded.
            let emit_failures = Arc::new(AtomicU64::new(0));
            let emitter: scheduler::TelemetryEmitter = Arc::new(move |payload| {
                use tauri::Emitter;
                if let Err(error) = emit_handle.emit(scheduler::TELEMETRY_EVENT, payload) {
                    let count = emit_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if count == 0 || count.is_multiple_of(100) {
                        eprintln!("telemetry emit failed ({} so far): {error}", count + 1);
                    }
                }
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
                capabilities: Arc::new(Mutex::new(capabilities)),
                capabilities_revision: capabilities_revision.clone(),
                paused,
                tray: tray_controller,
                store,
                polling_secs,
                diagnostics,
                scheduler: scheduler_handle,
                history_writer: Mutex::new(Some(history_writer)),
                processes: Arc::new(Mutex::new(processes::ProcessManager::new())),
                scans: crate::storage::ScanRegistry::new(),
                history_dir: Mutex::new(Some(history_dir)),
                alerts: alert_runtime,
                remote: Mutex::new(None),
            });
            // Phase 08 startup: crash-recover recording sessions, then
            // apply the persisted remote configuration — a listener starts
            // only when the user opted in (loopback) AND a token exists.
            recordings::recover_interrupted_recordings(&handle.state::<AppState>());
            remote::apply_remote_at_startup(&handle.state::<AppState>());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().try_state::<AppState>();
                let close_to_tray = state
                    .as_ref()
                    .and_then(|state| state.store.load_settings().ok())
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(false);
                if close_to_tray {
                    // Phase 06: the tray keeps sampling alive; closing the
                    // window hides it instead of exiting.
                    api.prevent_close();
                    let _ = window.hide();
                } else if let Some(state) = state {
                    state.scheduler.stop();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_capabilities,
            commands::reprobe_capabilities,
            commands::get_gpu_info,
            commands::get_settings,
            commands::update_settings,
            commands::reset_settings,
            commands::get_diagnostics,
            commands::get_process_page,
            commands::request_process_action,
            commands::get_history,
            commands::clear_history,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_scan_status,
            commands::get_scan_children,
            commands::move_to_trash,
            commands::open_in_manager,
            commands::lookup_external_ip,
            alerts::get_alert_rules,
            alerts::create_alert_rule,
            alerts::update_alert_rule,
            alerts::delete_alert_rule,
            alerts::get_alert_events,
            alerts::get_notification_state,
            alerts::send_test_notification,
            alerts::test_alert_rule,
            remote::get_remote_status,
            remote::configure_remote,
            remote::remote_rotate_token,
            remote::remote_revoke_token,
            recordings::start_recording,
            recordings::stop_recording,
            recordings::list_recordings,
            recordings::export_csv,
            recordings::export_recording_csv,
        ])
        .build(tauri::generate_context!())
        .expect("error while building APP_NAME")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.scheduler.stop();
                    commands::shutdown_history(&state);
                    remote::shutdown_remote(&state);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The user-facing capability report must cover every metric family in
    /// the contract exactly once, with no cross-compile leaks (FX-021).
    /// Hardware-conditional families appear as reasoned `unsupported` rows
    /// rather than absences.
    #[test]
    fn merged_capability_report_covers_every_family_exactly_once() {
        let report = build_capability_report("test-host", "2026-10-19T00:00:00Z".to_string());
        let mut actual: Vec<&str> = report
            .capabilities
            .iter()
            .map(|capability| capability.metric_id.as_str())
            .collect();
        actual.sort_unstable();
        let mut expected: Vec<&str> = monitor_core::families::ALL_FAMILIES.to_vec();
        expected.sort_unstable();
        assert_eq!(actual, expected, "one row per family, no extras");
        assert!(report
            .capabilities
            .iter()
            .all(|c| c.reason_code.as_deref() != Some(monitor_core::reasons::WRONG_PLATFORM)));
        // Every row carries a source label, and every *unsupported* row
        // carries an explanation — the honesty requirement; available rows
        // may be bare.
        assert!(report.capabilities.iter().all(|c| !c.source.is_empty()));
        assert!(report.capabilities.iter().all(|c| c.state
            != monitor_core::Availability::Unsupported
            || c.explanation
                .as_deref()
                .is_some_and(|text| !text.is_empty())));
    }

    /// Crash entries are one line each, name the thread and truncate long
    /// payloads (crash handling, phase 09 §2.3).
    #[test]
    fn crash_log_line_is_single_line_and_truncated() {
        let line = crash_log_line(Some("main"), "short failure", Some("src/lib.rs:10:5"));
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.contains("thread="));
        assert!(line.contains("src/lib.rs:10:5"));

        let long = "x".repeat(2_000);
        let truncated = crash_log_line(None, &long, None);
        assert!(truncated.chars().count() < 600);
        assert!(truncated.contains("…"));
        assert!(truncated.contains("<unnamed>"));
    }

    /// The crash log appends across runs and never rewrites history.
    #[test]
    fn append_crash_entry_appends_without_truncating() {
        let dir = std::env::temp_dir().join(format!("hostlume-crash-{}", uuid::Uuid::new_v4()));
        let path = dir.join("crash.log");
        append_crash_entry(&path, "first\n");
        append_crash_entry(&path, "second\n");
        let contents = std::fs::read_to_string(&path).expect("crash log readable");
        assert_eq!(contents, "first\nsecond\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
