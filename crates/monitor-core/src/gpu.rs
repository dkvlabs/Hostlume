//! Graphics adapter and display metadata (FX-010). This is *identity*
//! information probed on demand — not part of the polled metric stream —
//! so live-utilization availability stays honest in the snapshot while
//! static device facts are served through a typed IPC command.
//!
//! Unified-memory rule (metrics contract §3): `vram_bytes` is `None`
//! whenever the OS does not expose a dedicated-VRAM counter. An
//! `is_unified_memory: true` adapter must never grow a fabricated VRAM
//! number; the UI labels shared memory as shared.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuDisplay {
    /// OS-reported display name (e.g. "Color LCD").
    pub name: String,
    /// OS-reported resolution string, verbatim (e.g. "2056 x 1329 @ 120.00Hz").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default)]
    pub is_main: bool,
    #[serde(default)]
    pub is_online: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuAdapter {
    /// Stable entity key (`gpu:<index>` in probe order; adapters re-probed
    /// by the same command keep their order on an unchanged machine).
    pub id: String,
    /// Driver/OS-reported adapter name.
    pub name: String,
    /// Vendor label as reported (e.g. "sppci_vendor_Apple", "0x8086 (Intel)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// True when the adapter shares system memory (Apple Silicon, iGPU
    /// reporting unified memory). Dedicated VRAM is meaningless here.
    #[serde(default)]
    pub is_unified_memory: bool,
    /// Dedicated VRAM in bytes, only when the OS reports a real counter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_bytes: Option<u64>,
    pub displays: Vec<GpuDisplay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfoReport {
    pub generated_at_utc: String,
    pub adapters: Vec<GpuAdapter>,
}

impl GpuInfoReport {
    pub fn new(generated_at_utc: impl Into<String>, adapters: Vec<GpuAdapter>) -> Self {
        Self {
            generated_at_utc: generated_at_utc.into(),
            adapters,
        }
    }
}
