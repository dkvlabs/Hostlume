//! macOS GPU adapter/display metadata provider (FX-010, phase 05 §2.2).
//!
//! Identity facts come from `system_profiler SPDisplaysDataType -json` —
//! Apple's public, rootless reporting tool. This is *static* identity
//! (adapter name/vendor, displays, resolutions); live utilization has no
//! public non-root source and stays `unsupported` in the metric stream.
//!
//! Unified-memory rule: Apple adapters are marked `is_unified_memory` and
//! never get a fabricated VRAM figure. On Intel Macs with discrete GPUs a
//! reported `spdisplays_vram` string is parsed when present.

use monitor_core::{GpuAdapter, GpuDisplay};

/// Pure parser over the profiler JSON — fixture-testable on every OS.
pub fn parse_displays_json(text: &str) -> Result<Vec<GpuAdapter>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("invalid profiler JSON: {err}"))?;
    let entries = value
        .get("SPDisplaysDataType")
        .and_then(|v| v.as_array())
        .ok_or("missing SPDisplaysDataType")?;
    let mut adapters = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let name = entry
            .get("_name")
            .and_then(|v| v.as_str())
            .unwrap_or("GPU")
            .to_string();
        let vendor = entry
            .get("spdisplays_vendor")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let is_unified_memory = vendor
            .as_deref()
            .map(|vendor| vendor.contains("Apple"))
            .unwrap_or(false);
        // `spdisplays_vram` only appears on adapters that *have* dedicated
        // VRAM (Intel discrete); parse "1536 MB" style strings.
        let vram_bytes = entry
            .get("spdisplays_vram")
            .and_then(|v| v.as_str())
            .and_then(parse_vram_string);
        let mut displays = Vec::new();
        if let Some(ndrvs) = entry.get("spdisplays_ndrvs").and_then(|v| v.as_array()) {
            for display in ndrvs {
                displays.push(GpuDisplay {
                    name: display
                        .get("_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Display")
                        .to_string(),
                    resolution: display
                        .get("_spdisplays_resolution")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    is_main: display.get("spdisplays_main").and_then(|v| v.as_str())
                        == Some("spdisplays_yes"),
                    is_online: display.get("spdisplays_online").and_then(|v| v.as_str())
                        == Some("spdisplays_yes"),
                });
            }
        }
        adapters.push(GpuAdapter {
            id: format!("gpu:{index}"),
            name,
            vendor,
            is_unified_memory,
            vram_bytes,
            displays,
        });
    }
    Ok(adapters)
}

/// `"1536 MB"` / `"8 GB"` → bytes. Returns None for anything unparsable —
/// never a guess.
pub fn parse_vram_string(text: &str) -> Option<u64> {
    let text = text.trim();
    let (value, unit) = text.split_once(' ')?;
    let value: f64 = value.trim().parse().ok()?;
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "kb" => 1024.0,
        "mb" => 1024.0 * 1024.0,
        "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * multiplier) as u64)
}

/// Probe the live machine. The program name and arguments are literals at
/// the spawn site by policy; no variable ever reaches a process.
pub fn adapters() -> Result<Vec<GpuAdapter>, String> {
    let program = "system_profiler";
    let output = std::process::Command::new(program)
        .args(["SPDisplaysDataType", "-json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|err| format!("system_profiler failed: {err}"))?;
    if !output.status.success() {
        return Err("system_profiler exited nonzero".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_displays_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modeled on the live probe of this MacBook (Phase 00 + Phase 05):
    /// Apple M3 Max, internal display, no VRAM key — unified memory.
    const FIXTURE_APPLE_SILICON: &str = r#"{
        "SPDisplaysDataType": [
            {
                "_name": "Apple M3 Max",
                "spdisplays_mtlgpufamilysupport": "spdisplays_metal4",
                "spdisplays_ndrvs": [
                    {
                        "_name": "Color LCD",
                        "_spdisplays_pixels": "4112 x 2658",
                        "_spdisplays_resolution": "2056 x 1329 @ 120.00Hz",
                        "spdisplays_connection_type": "spdisplays_internal",
                        "spdisplays_main": "spdisplays_yes",
                        "spdisplays_online": "spdisplays_yes"
                    }
                ],
                "spdisplays_vendor": "sppci_vendor_Apple",
                "sppci_bus": "spdisplays_builtin",
                "sppci_cores": "30",
                "sppci_device_type": "spdisplays_gpu",
                "sppci_model": "Apple M3 Max"
            }
        ]
    }"#;

    /// Intel Mac with a discrete GPU: a VRAM string exists and must parse;
    /// the adapter must NOT be flagged unified-memory.
    const FIXTURE_INTEL_DISCRETE: &str = r#"{
        "SPDisplaysDataType": [
            {
                "_name": "AMD Radeon Pro 5300M",
                "spdisplays_vendor": "sppci_vendor_amd",
                "spdisplays_vram_shared": "spdisplays_yes",
                "spdisplays_vram": "4 GB",
                "spdisplays_ndrvs": [
                    {
                        "_name": "DELL U2723QE",
                        "_spdisplays_resolution": "3840 x 2160 @ 60.00Hz",
                        "spdisplays_main": "spdisplays_yes",
                        "spdisplays_online": "spdisplays_yes"
                    }
                ]
            }
        ]
    }"#;

    #[test]
    fn apple_silicon_fixture_is_unified_and_vram_free() {
        let adapters = parse_displays_json(FIXTURE_APPLE_SILICON).expect("parses");
        assert_eq!(adapters.len(), 1);
        let adapter = &adapters[0];
        assert_eq!(adapter.name, "Apple M3 Max");
        assert!(adapter.is_unified_memory);
        assert_eq!(
            adapter.vram_bytes, None,
            "unified-memory adapters must never grow a VRAM number"
        );
        assert_eq!(adapter.displays.len(), 1);
        let display = &adapter.displays[0];
        assert_eq!(display.name, "Color LCD");
        assert_eq!(
            display.resolution.as_deref(),
            Some("2056 x 1329 @ 120.00Hz")
        );
        assert!(display.is_main && display.is_online);
    }

    #[test]
    fn intel_discrete_fixture_keeps_real_vram() {
        let adapters = parse_displays_json(FIXTURE_INTEL_DISCRETE).expect("parses");
        let adapter = &adapters[0];
        assert!(!adapter.is_unified_memory);
        assert_eq!(adapter.vram_bytes, Some(4 * 1024 * 1024 * 1024));
    }

    #[test]
    fn vram_strings_parse_or_refuse() {
        assert_eq!(parse_vram_string("1536 MB"), Some(1536 * 1024 * 1024));
        assert_eq!(parse_vram_string("8 GB"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_vram_string("unknown"), None);
        assert_eq!(parse_vram_string(""), None);
        assert_eq!(parse_vram_string("12 TB"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn live_probe_reports_this_machines_adapters() {
        let adapters = adapters().expect("live probe");
        assert!(!adapters.is_empty());
        assert!(adapters[0].displays.iter().any(|display| display.is_online));
    }
}
