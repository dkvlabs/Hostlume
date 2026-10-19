//! Windows GPU adapter/display metadata provider (FX-010, phase 05 §2.2).
//!
//! Adapter identity and dedicated-VRAM *capacity* come from DXGI
//! (`CreateDXGIFactory1` → `EnumAdapters1` → `GetDesc1`), outputs from
//! `EnumOutputs`/`GetDesc`. This is static identity for the `get_gpu_info`
//! command. Live GPU counters are intentionally **not** reported by this
//! build (see the GPU collector's `NOT_VALIDATED_ON_HARDWARE` capability
//! row): the D3DKMT statistics surface needs validation on real Windows
//! hardware before any number ships.
//!
//! The pure normalization helpers are cfg-free and fixture-tested on every
//! OS; the DXGI calls are compiled by the Windows CI runner.

use monitor_core::{GpuAdapter, GpuDisplay};

/// Nul-terminated UTF-16 (DXGI `Description` buffers) → Rust string.
pub fn parse_utf16_nul(buffer: &[u16]) -> String {
    let len = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

/// One normalized output row produced by the DXGI scan (kept as a plain
/// struct so the pure builder can be fixture-tested without COM).
#[derive(Debug, Clone, PartialEq)]
pub struct OutputFacts {
    pub name: String,
    /// Desktop rectangle size (right-left, bottom-top) in pixels.
    pub width: i64,
    pub height: i64,
    pub attached_to_desktop: bool,
}

/// Pure adapter assembly, fixture-tested on every OS.
pub fn build_adapter(
    index: u32,
    name: String,
    vendor_id: u32,
    dedicated_vram_bytes: u64,
    outputs: Vec<OutputFacts>,
) -> GpuAdapter {
    GpuAdapter {
        id: format!("gpu:{index}"),
        name,
        // PCI vendor ids are stable; "0x10DE" style keeps the raw fact.
        vendor: Some(format!("0x{vendor_id:04X}")),
        // Windows/DXGI does not expose a "unified memory" concept; the UI
        // labels the figure as dedicated video memory as reported.
        is_unified_memory: false,
        vram_bytes: (dedicated_vram_bytes > 0).then_some(dedicated_vram_bytes),
        displays: outputs
            .into_iter()
            .map(|output| GpuDisplay {
                name: output.name,
                resolution: (output.width > 0 && output.height > 0)
                    .then(|| format!("{} x {}", output.width, output.height)),
                // DXGI does not identify the primary output; `main` stays
                // false rather than guessing, `online` follows attach state.
                is_main: false,
                is_online: output.attached_to_desktop,
            })
            .collect(),
    }
}

#[cfg(windows)]
pub fn adapters() -> Result<Vec<GpuAdapter>, String> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_OUTPUT_DESC,
    };

    let created = unsafe { CreateDXGIFactory1() };
    let factory: IDXGIFactory1 = created.map_err(|err| format!("CreateDXGIFactory1: {err}"))?;
    let mut adapters = Vec::new();
    for index in 0..16u32 {
        let next = unsafe { factory.EnumAdapters1(index) };
        // DXGI_ERROR_NOT_FOUND ends the enumeration.
        let Ok(adapter) = next else {
            break;
        };
        let desc: DXGI_ADAPTER_DESC1 =
            unsafe { adapter.GetDesc1().map_err(|err| format!("GetDesc1: {err}")) }?;
        let mut outputs = Vec::new();
        for output_index in 0..16u32 {
            let next = unsafe { adapter.EnumOutputs(output_index) };
            let Ok(output) = next else {
                break;
            };
            let probed = unsafe { output.GetDesc() };
            let Ok(desc) = probed else {
                continue;
            };
            outputs.push(OutputFacts {
                name: parse_utf16_nul(&desc.DeviceName),
                width: (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as i64,
                height: (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as i64,
                attached_to_desktop: desc.AttachedToDesktop.as_bool(),
            });
        }
        adapters.push(build_adapter(
            index,
            parse_utf16_nul(&desc.Description),
            desc.VendorId,
            desc.DedicatedVideoMemory as u64,
            outputs,
        ));
    }
    Ok(adapters)
}

#[cfg(not(windows))]
pub fn adapters() -> Result<Vec<GpuAdapter>, String> {
    Err("compiled for a different target OS".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_buffers_decode_until_nul() {
        let mut buffer: Vec<u16> = "AMD Radeon\0".encode_utf16().collect();
        buffer.extend_from_slice(&[0xEEEE; 8]);
        assert_eq!(parse_utf16_nul(&buffer), "AMD Radeon");
        assert_eq!(parse_utf16_nul(&[]), "");
    }

    #[test]
    fn adapter_normalization_keeps_vram_and_outputs() {
        let adapter = build_adapter(
            0,
            "NVIDIA GeForce RTX 4060".to_string(),
            0x10DE,
            8 * 1024 * 1024 * 1024,
            vec![OutputFacts {
                name: "\\\\.\\DISPLAY1".to_string(),
                width: 3840,
                height: 2160,
                attached_to_desktop: true,
            }],
        );
        assert_eq!(adapter.id, "gpu:0");
        assert_eq!(adapter.vendor.as_deref(), Some("0x10DE"));
        assert_eq!(adapter.vram_bytes, Some(8 * 1024 * 1024 * 1024));
        assert!(!adapter.is_unified_memory);
        assert_eq!(adapter.displays.len(), 1);
        assert_eq!(
            adapter.displays[0].resolution.as_deref(),
            Some("3840 x 2160")
        );
        assert!(adapter.displays[0].is_online);
        assert!(!adapter.displays[0].is_main, "primary is unknown via DXGI");
    }

    #[test]
    fn zero_vram_is_absent_not_zero_reported() {
        let adapter = build_adapter(1, "Microsoft Basic Display".into(), 0x1414, 0, vec![]);
        assert_eq!(adapter.vram_bytes, None);
        assert!(adapter.displays.is_empty());
    }
}
