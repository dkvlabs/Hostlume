//! Linux GPU adapter/display metadata provider (FX-010, phase 05 §2.2).
//!
//! Identity facts from DRM sysfs: `/sys/class/drm/cardN/device/{vendor,
//! device}` PCI ids and the connector tree (`cardN-DP-1` status/modes).
//! Live busy/VRAM counters are the polled collector's job, not this
//! provider's. Everything here is plain file parsing — fixture-testable
//! on any OS.

use monitor_core::{GpuAdapter, GpuDisplay};

/// `"0x1002\n"` → `"0x1002"`.
pub fn parse_pci_id(text: &str) -> Option<String> {
    let text = text.trim();
    text.starts_with("0x").then(|| text.to_string())
}

/// First preferred mode from a connector `modes` file:
/// `"1920x1080 60.00 148.500 1920 2008 ..."` → `"1920x1080 @ 60Hz"`.
pub fn parse_mode_line(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let resolution = parts.next()?;
    let refresh: f64 = parts.next()?.parse().ok()?;
    Some(format!("{resolution} @ {refresh:.0}Hz"))
}

/// `(connector_name, connected, first_mode_line)` → display row.
pub fn build_display(name: String, connected: bool, first_mode: Option<&str>) -> GpuDisplay {
    GpuDisplay {
        name,
        resolution: if connected {
            first_mode.and_then(parse_mode_line)
        } else {
            None
        },
        is_main: false, // kernel sysfs does not expose the primary display
        is_online: connected,
    }
}

/// Probe the live machine by reading DRM sysfs. No subprocess involved.
pub fn adapters() -> Result<Vec<GpuAdapter>, String> {
    let mut adapters = Vec::new();
    let entries =
        std::fs::read_dir("/sys/class/drm").map_err(|err| format!("no DRM tree: {err}"))?;
    let mut cards: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let is_card = name.starts_with("card")
                && name["card".len()..].chars().all(|c| c.is_ascii_digit());
            is_card.then(|| (name, entry.path()))
        })
        .collect();
    cards.sort();
    for (index, (name, path)) in cards.into_iter().enumerate() {
        let device = path.join("device");
        let vendor = std::fs::read_to_string(device.join("vendor"))
            .ok()
            .and_then(|text| parse_pci_id(&text));
        let vendor_display = vendor.map(|id| match id.as_str() {
            "0x1002" => "0x1002 (AMD)".to_string(),
            "0x8086" => "0x8086 (Intel)".to_string(),
            "0x10DE" => "0x10DE (NVIDIA)".to_string(),
            other => other.to_string(),
        });
        let mut displays = Vec::new();
        if let Ok(connectors) = std::fs::read_dir(&path) {
            let mut connector_names: Vec<String> = connectors
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| name.contains('-'))
                .collect();
            connector_names.sort();
            for connector in connector_names {
                let connector_path = path.join(&connector);
                let connected = std::fs::read_to_string(connector_path.join("status"))
                    .map(|text| text.trim() == "connected")
                    .unwrap_or(false);
                let first_mode = std::fs::read_to_string(connector_path.join("modes"))
                    .ok()
                    .and_then(|text| text.lines().next().map(str::to_string));
                displays.push(build_display(connector, connected, first_mode.as_deref()));
            }
        }
        adapters.push(GpuAdapter {
            id: format!("gpu:card{index}"),
            name,
            vendor: vendor_display,
            // Whether the driver reports shared memory cannot be inferred
            // from PCI ids; false keeps the display wording accurate.
            is_unified_memory: false,
            vram_bytes: None,
            displays,
        });
    }
    Ok(adapters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pci_ids_and_modes_parse() {
        assert_eq!(parse_pci_id("0x1002\n"), Some("0x1002".to_string()));
        assert_eq!(parse_pci_id("junk"), None);
        assert_eq!(
            parse_mode_line(
                "1920x1080 60.00 148.500 1920 2008 2052 2200 1080 1084 1089 1125 -hsync -vsync"
            ),
            Some("1920x1080 @ 60Hz".to_string())
        );
        assert_eq!(parse_mode_line(""), None);
        assert_eq!(parse_mode_line("1920x1080"), None);
    }

    #[test]
    fn disconnected_connector_has_no_resolution() {
        let display = build_display("DP-1".to_string(), false, Some("1920x1080 60.00 1"));
        assert!(!display.is_online);
        assert_eq!(display.resolution, None);
        let connected = build_display("HDMI-A-1".to_string(), true, Some("3840x2160 60.00 1"));
        assert!(connected.is_online);
        assert_eq!(connected.resolution.as_deref(), Some("3840x2160 @ 60Hz"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn live_probe_reads_the_drm_tree() {
        // May be empty on CI runners without DRM devices — absence is a
        // fact, not an error.
        let adapters = adapters().expect("probe ok");
        for adapter in &adapters {
            assert!(adapter.id.starts_with("gpu:card"));
        }
    }
}
