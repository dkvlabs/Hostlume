//! APP_NAME desktop shell — Tauri 2 entry point (Phase 01 foundation).
//!
//! Security posture (security-privacy.md): strict CSP, `core:default`
//! capability only, no shell/filesystem plugins, all command inputs
//! validated in Rust, no remote listener, no privileged helper.

mod commands;
mod scheduler;
mod settings;
mod settings_store;
mod state;

use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, RwLock};

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
                emitter,
            );

            app.manage(AppState {
                latest,
                capabilities,
                store,
                polling_secs,
                scheduler: scheduler_handle,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Phase 01: closing the main window exits the app. The
            // close-to-tray preference is stored but only becomes active
            // with the tray integration (Phase 06); until then exit is the
            // honest behavior and is documented in Settings.
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running APP_NAME");
}
