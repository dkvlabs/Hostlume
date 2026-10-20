//! Tray controller (FX-015, phase 06 §2.1–2.4).
//!
//! One tray icon + native menu fed by the **existing** normalized snapshot
//! bus — there is no second polling loop (phase 06 §2.1 is a hard rule):
//! the scheduler's snapshot tap invokes [`TrayController::update`], which
//! computes display strings and repaints only what changed (phase 06 §2.3).
//!
//! Honesty rules apply verbatim: an unavailable value renders as "—", never
//! a zero; the network row shows the **dominant interface by current
//! traffic** (a defined aggregation — never a sum of physical + virtual
//! counters that would double-count tunnels).
//!
//! All native menu mutation is dispatched to the main thread (`muda`
//! requirement on macOS); the tap thread only formats strings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use monitor_core::SystemSnapshot;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::settings::TrayConfig;

pub const MENU_OPEN: &str = "tray.open";
pub const MENU_COMPACT: &str = "tray.compact";
pub const MENU_PAUSE: &str = "tray.pause";
pub const MENU_SETTINGS: &str = "tray.settings";
pub const MENU_QUIT: &str = "tray.quit";

/// What the tray would display for one snapshot. Pure and unit-testable;
/// `None` means "unavailable — render the dash".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayValues {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub network: Option<String>,
    /// macOS menu-bar icon text (compact CPU percentage).
    pub title: Option<String>,
}

/// SI-decimal human rate (requirements: SI prefixes for rates).
pub fn format_rate(bytes_per_second: f64, rate_bits: bool) -> String {
    let (value, unit) = if rate_bits {
        (bytes_per_second * 8.0, "b")
    } else {
        (bytes_per_second, "B")
    };
    for (factor, prefix) in [(1e9, "G"), (1e6, "M"), (1e3, "k")] {
        if value >= factor {
            return format!("{:.1} {}{}/s", value / factor, prefix, unit);
        }
    }
    format!("{:.0} {}/s", value, unit)
}

/// Binary-prefix human size (requirements: binary for capacities).
pub fn format_memory_gib(gib: f64) -> String {
    if gib >= 1024.0 {
        format!("{:.1} TiB", gib / 1024.0)
    } else {
        format!("{:.1} GiB", gib)
    }
}

/// The busiest interface right now (max rx+tx among available rate
/// readings). Returns `(label, rx, tx)`; `None` when nothing reports.
pub fn dominant_interface(snapshot: &SystemSnapshot) -> Option<(String, f64, f64)> {
    let mut best: Option<(String, f64, f64, f64)> = None;
    for rx in &snapshot.metrics {
        if rx.metric_id != monitor_core::families::NET_INTERFACE_RX_RATE
            || rx.availability != monitor_core::Availability::Available
        {
            continue;
        }
        let Some(monitor_core::MetricValue::Number(rx_value)) = rx.value else {
            continue;
        };
        let tx = snapshot.metrics.iter().find(|candidate| {
            candidate.metric_id == monitor_core::families::NET_INTERFACE_TX_RATE
                && candidate.entity_id == rx.entity_id
                && candidate.availability == monitor_core::Availability::Available
        });
        let Some(monitor_core::MetricValue::Number(tx_value)) = tx.and_then(|t| t.value.as_ref())
        else {
            continue;
        };
        let total = rx_value + tx_value;
        if best
            .as_ref()
            .is_none_or(|(_, _, _, best_total)| total > *best_total)
        {
            // `if:<name>:<generation>` → the name (generation dropped).
            let label = rx
                .entity_id
                .strip_prefix("if:")
                .map(|rest| match rest.rfind(':') {
                    Some(last) => &rest[..last],
                    None => rest,
                })
                .unwrap_or(&rx.entity_id)
                .to_string();
            best = Some((label, rx_value, *tx_value, total));
        }
    }
    best.map(|(label, rx, tx, _)| (label, rx, tx))
}

fn available_number(snapshot: &SystemSnapshot, metric_id: &str) -> Option<f64> {
    let reading = snapshot.metrics.iter().find(|reading| {
        reading.metric_id == metric_id
            && reading.availability == monitor_core::Availability::Available
    });
    let value = reading.and_then(|r| r.value.as_ref());
    match value {
        Some(monitor_core::MetricValue::Number(value)) => Some(*value),
        _ => None,
    }
}

impl TrayValues {
    pub fn from_snapshot(snapshot: &SystemSnapshot, config: &TrayConfig, rate_bits: bool) -> Self {
        let mut values = Self::default();
        if config.show_cpu {
            values.cpu = Some(
                available_number(snapshot, monitor_core::families::CPU_TOTAL_USAGE)
                    .map(|cpu| format!("CPU {cpu:.0}%"))
                    .unwrap_or_else(|| "CPU —".to_string()),
            );
            if config.show_title {
                values.title = available_number(snapshot, monitor_core::families::CPU_TOTAL_USAGE)
                    .map(|cpu| format!("{cpu:.0}%"));
            }
        } else {
            values.cpu = Some("CPU —".to_string());
        }
        if config.show_memory {
            values.memory = Some(
                match (
                    available_number(snapshot, monitor_core::families::MEMORY_PHYSICAL_USED),
                    available_number(snapshot, monitor_core::families::MEMORY_PHYSICAL_TOTAL),
                ) {
                    (Some(used), Some(total)) if total > 0.0 => format!(
                        "Memory {} / {}",
                        format_memory_gib(used / 1024.0 / 1024.0 / 1024.0),
                        format_memory_gib(total / 1024.0 / 1024.0 / 1024.0)
                    ),
                    _ => "Memory —".to_string(),
                },
            );
        } else {
            values.memory = Some("Memory —".to_string());
        }
        if config.show_network {
            values.network = Some(match dominant_interface(snapshot) {
                Some((label, rx, tx)) => format!(
                    "↓ {} · ↑ {} · {label}",
                    format_rate(rx, rate_bits),
                    format_rate(tx, rate_bits)
                ),
                None => "Network —".to_string(),
            });
        } else {
            values.network = Some("Network —".to_string());
        }
        values
    }
}

/// One tray icon + its native menu. Constructed once in setup; updated
/// from the snapshot tap.
pub struct TrayController {
    app: AppHandle<Wry>,
    /// `(config, rate_bits)` behind a mutex so the settings command can
    /// apply changes on a shared controller.
    config: Mutex<(TrayConfig, bool)>,
    paused: std::sync::Arc<AtomicBool>,
    previous: Mutex<TrayValues>,
    tray: tauri::tray::TrayIcon<Wry>,
    cpu_item: MenuItem<Wry>,
    memory_item: MenuItem<Wry>,
    network_item: MenuItem<Wry>,
    pause_item: MenuItem<Wry>,
}

impl TrayController {
    /// Build the icon + menu. `icon_bytes` must be PNG bytes (the
    /// `image-png` feature decodes them); failures are typed, never silent.
    pub fn build(
        app: AppHandle<Wry>,
        config: TrayConfig,
        rate_bits: bool,
        paused: std::sync::Arc<AtomicBool>,
        icon_bytes: &'static [u8],
    ) -> Result<Self, tauri::Error> {
        let paused_now = paused.load(Ordering::Relaxed);
        let cpu_item = MenuItem::with_id(&app, "tray.value.cpu", "CPU —", false, None::<&str>)?;
        let memory_item =
            MenuItem::with_id(&app, "tray.value.memory", "Memory —", false, None::<&str>)?;
        let network_item =
            MenuItem::with_id(&app, "tray.value.network", "Network —", false, None::<&str>)?;
        let pause_item = MenuItem::with_id(
            &app,
            MENU_PAUSE,
            if paused_now {
                "Resume sampling"
            } else {
                "Pause sampling"
            },
            true,
            None::<&str>,
        )?;
        let open_item = MenuItem::with_id(&app, MENU_OPEN, "Open Hostlume", true, None::<&str>)?;
        let compact_item =
            MenuItem::with_id(&app, MENU_COMPACT, "Compact monitor", true, None::<&str>)?;
        let settings_item =
            MenuItem::with_id(&app, MENU_SETTINGS, "Settings…", true, None::<&str>)?;
        let quit_item = MenuItem::with_id(&app, MENU_QUIT, "Quit Hostlume", true, None::<&str>)?;
        let separator = PredefinedMenuItem::separator(&app)?;
        let separator2 = PredefinedMenuItem::separator(&app)?;
        let menu = Menu::with_items(
            &app,
            &[
                &cpu_item,
                &memory_item,
                &network_item,
                &separator,
                &open_item,
                &compact_item,
                &pause_item,
                &settings_item,
                &separator2,
                &quit_item,
            ],
        )?;
        let paused_for_menu = paused.clone();
        let icon = tauri::image::Image::from_bytes(icon_bytes)?;
        let tray = TrayIconBuilder::with_id("main-tray")
            .icon(icon)
            .menu(&menu)
            // Left click opens the menu on all platforms (predictable
            // left/right semantics; the phase doc asks for explicitness).
            .show_menu_on_left_click(true)
            .tooltip("Hostlume")
            .on_menu_event(move |app, event| {
                handle_menu(app, event.id().as_ref(), &paused_for_menu);
            })
            .build(&app)?;
        Ok(Self {
            app,
            config: Mutex::new((config, rate_bits)),
            paused,
            previous: Mutex::new(TrayValues::default()),
            tray,
            cpu_item,
            memory_item,
            network_item,
            pause_item,
        })
    }

    /// Called from the snapshot tap (scheduler thread): compute display
    /// strings and repaint only what changed, on the main thread.
    pub fn update(&self, snapshot: &SystemSnapshot) {
        let (config, rate_bits) = *self
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let values = TrayValues::from_snapshot(snapshot, &config, rate_bits);
        {
            let mut previous = self
                .previous
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *previous == values {
                return; // nothing displayed changed — no repaint
            }
            *previous = values.clone();
        }
        let cpu_item = self.cpu_item.clone();
        let memory_item = self.memory_item.clone();
        let network_item = self.network_item.clone();
        let tray = self.tray.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(text) = values.cpu {
                let _ = cpu_item.set_text(text);
            }
            if let Some(text) = values.memory {
                let _ = memory_item.set_text(text);
            }
            if let Some(text) = values.network {
                let _ = network_item.set_text(text);
            }
            let _ = tray.set_title(values.title);
        });
    }

    /// Swap the persisted configuration; the next snapshot applies it and
    /// icon visibility follows `enabled` (toggling needs no restart).
    pub fn apply_config(&self, config: TrayConfig, rate_bits: bool) {
        *self
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (config, rate_bits);
        // Force one full repaint with the new configuration.
        *self
            .previous
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = TrayValues::default();
        let _ = self.tray.set_visible(config.enabled);
    }

    /// Update the pause row after a toggle.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        let pause_item = self.pause_item.clone();
        let _ = self.app.run_on_main_thread(move || {
            let _ = pause_item.set_text(if paused {
                "Resume sampling"
            } else {
                "Pause sampling"
            });
        });
    }
}

/// Settings deep-link: the frontend listens and switches pages.
pub const NAVIGATE_EVENT: &str = "navigate://page";

/// Open (or create) the compact floating monitor — the documented fallback
/// for desktop environments without a tray (phase 06 §2.6) and a manual
/// option from the tray menu everywhere.
pub fn toggle_compact_window(app: &AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("compact") {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
    let _ = tauri::WebviewWindowBuilder::new(
        app,
        "compact",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Hostlume")
    .inner_size(300.0, 190.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .initialization_script("window.__HOSTLUME_COMPACT__ = true;")
    .build();
}

/// Tray menu actions (phase 06 §2.2: open, pause/resume, settings, quit;
/// plus the compact-monitor fallback). Runs on the main thread.
fn handle_menu(app: &AppHandle<Wry>, id: &str, paused: &AtomicBool) {
    use tauri::Emitter;
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    match id {
        MENU_OPEN => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        MENU_COMPACT => toggle_compact_window(app),
        MENU_PAUSE => {
            let now_paused = !paused.load(Ordering::Relaxed);
            state.scheduler.set_paused(now_paused);
            state.paused.store(now_paused, Ordering::Relaxed);
            if let Some(tray) = state.tray.as_ref() {
                tray.set_paused(now_paused);
            }
        }
        MENU_SETTINGS => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit(NAVIGATE_EVENT, "settings");
        }
        MENU_QUIT => {
            // Actual Quit (phase 06 §2.9): stop collector, flush history,
            // exit; RunEvent::Exit performs the same cleanup idempotently.
            state.scheduler.stop();
            crate::commands::shutdown_history(&state);
            app.exit(0);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::{MetricReading, MetricUnit, MetricValue};

    fn reading(metric_id: &str, entity_id: &str, value: f64) -> MetricReading {
        MetricReading {
            metric_id: metric_id.to_string(),
            entity_id: entity_id.to_string(),
            value: Some(MetricValue::Number(value)),
            unit: MetricUnit::Percent,
            availability: monitor_core::Availability::Available,
            source: "test".to_string(),
            collected_at_utc: "2026-10-19T00:00:00Z".to_string(),
            monotonic_sequence: 1,
            reason_code: None,
            human_message_key: None,
            precision: None,
        }
    }

    fn unavailable(metric_id: &str, entity_id: &str) -> MetricReading {
        MetricReading {
            availability: monitor_core::Availability::TemporarilyUnavailable,
            value: None,
            reason_code: Some("WARMUP".to_string()),
            unit: MetricUnit::BytesPerSecond,
            ..reading(metric_id, entity_id, 0.0)
        }
    }

    fn snapshot(metrics: Vec<MetricReading>) -> SystemSnapshot {
        let mut snapshot = SystemSnapshot::empty("h", "g", 1u64, "t");
        snapshot.metrics = metrics;
        snapshot
    }

    #[test]
    fn rates_format_si_and_bits() {
        assert_eq!(format_rate(0.0, false), "0 B/s");
        assert_eq!(format_rate(999.0, false), "999 B/s");
        assert_eq!(format_rate(1_200.0, false), "1.2 kB/s");
        assert_eq!(format_rate(1_500_000.0, false), "1.5 MB/s");
        assert_eq!(format_rate(1_500_000.0, true), "12.0 Mb/s");
    }

    #[test]
    fn memory_formats_binary() {
        assert_eq!(format_memory_gib(12.34), "12.3 GiB");
        assert_eq!(format_memory_gib(36.0), "36.0 GiB");
        assert_eq!(format_memory_gib(2048.0), "2.0 TiB");
    }

    #[test]
    fn dominant_interface_picks_busiest_and_never_sums_adapters() {
        let snapshot = snapshot(vec![
            reading(
                monitor_core::families::NET_INTERFACE_RX_RATE,
                "if:en0:7",
                100.0,
            ),
            reading(
                monitor_core::families::NET_INTERFACE_TX_RATE,
                "if:en0:7",
                50.0,
            ),
            reading(
                monitor_core::families::NET_INTERFACE_RX_RATE,
                "if:utun8:20",
                900.0,
            ),
            reading(
                monitor_core::families::NET_INTERFACE_TX_RATE,
                "if:utun8:20",
                10.0,
            ),
        ]);
        let (label, rx, tx) = dominant_interface(&snapshot).expect("dominant");
        assert_eq!(label, "utun8");
        assert_eq!(rx, 900.0);
        assert_eq!(tx, 10.0);
        // No double counting: values are per-interface, never summed.
    }

    #[test]
    fn warmup_rows_render_dash_never_zero() {
        let snapshot = snapshot(vec![
            unavailable(monitor_core::families::NET_INTERFACE_RX_RATE, "if:en0:7"),
            unavailable(monitor_core::families::NET_INTERFACE_TX_RATE, "if:en0:7"),
        ]);
        let values = TrayValues::from_snapshot(&snapshot, &TrayConfig::default(), false);
        assert_eq!(values.network.as_deref(), Some("Network —"));
        assert_eq!(values.cpu.as_deref(), Some("CPU —"));
    }

    #[test]
    fn config_toggles_apply_and_off_rows_are_explicit() {
        let snapshot = snapshot(vec![reading(
            monitor_core::families::CPU_TOTAL_USAGE,
            "host",
            42.6,
        )]);
        let config = TrayConfig {
            show_cpu: false,
            show_memory: false,
            show_network: false,
            ..TrayConfig::default()
        };
        let values = TrayValues::from_snapshot(&snapshot, &config, false);
        assert_eq!(values.cpu.as_deref(), Some("CPU —"));
        assert_eq!(values.memory.as_deref(), Some("Memory —"));
        assert_eq!(values.network.as_deref(), Some("Network —"));
        assert_eq!(values.title, None, "hidden cpu row hides the title too");

        let config = TrayConfig {
            show_cpu: true,
            ..config
        };
        let values = TrayValues::from_snapshot(&snapshot, &config, false);
        assert_eq!(values.cpu.as_deref(), Some("CPU 43%"));
        assert_eq!(values.title.as_deref(), Some("43%"));
    }
}
