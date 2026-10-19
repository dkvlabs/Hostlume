//! Process collector and action manager (phase 03 §3B, FX-007).
//!
//! Identity is always **PID + start time** (contract §8): every action
//! re-queries the process and refuses when the start time differs (PID
//! reuse) or the process is gone. Refusals are first-class results, not
//! swallowed errors; no path ever elevates privileges.
//!
//! CPU semantics (verified against sysinfo 0.39.6 macOS source,
//! `compute_cpu_usage`): `Process::cpu_usage()` is **core-equivalent**
//! (can exceed 100% for multithreaded processes). We therefore report it
//! as `cpu_usage_cores` and derive `cpu_usage_host = cores_value /
//! logical_core_count` (ADR-0001). First page is a WARMUP: sysinfo needs
//! two refreshes for per-process CPU deltas, so CPU fields are null there
//! — the table still shows names and memory truthfully.

use monitor_core::{Availability, Clock, MetricCapability};
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

pub const COLLECTOR_ID: &str = "collector-processes";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProcessRow {
    pub pid: u32,
    /// Seconds since epoch (sysinfo start_time); identity half.
    #[serde(rename = "startTimeSec")]
    pub start_time_sec: u64,
    pub name: String,
    #[serde(rename = "rssBytes")]
    pub rss_bytes: u64,
    /// Core-equivalent percent (can exceed 100); null during warm-up.
    #[serde(rename = "cpuUsageCores")]
    pub cpu_usage_cores: Option<f64>,
    /// Host-normalized 0–100; null during warm-up.
    #[serde(rename = "cpuUsageHost")]
    pub cpu_usage_host: Option<f64>,
    pub status: String,
    #[serde(rename = "parentPid")]
    pub parent_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProcessPage {
    #[serde(rename = "collectedAtUtc")]
    pub collected_at_utc: String,
    /// True until the second refresh: CPU fields are null, everything else
    /// is real.
    #[serde(rename = "cpuWarmup")]
    pub cpu_warmup: bool,
    #[serde(rename = "logicalCores")]
    pub logical_cores: usize,
    pub rows: Vec<ProcessRow>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProcessActionRequest {
    pub pid: u32,
    #[serde(rename = "startTimeSec")]
    pub start_time_sec: u64,
    /// "graceful" (SIGTERM) or "force" (SIGKILL / TerminateProcess).
    pub mode: String,
    /// UI-provided correlation id for the audit trail; the Rust-side
    /// identity recheck is the security boundary, not this string.
    #[serde(rename = "confirmationId")]
    pub confirmation_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    Ok,
    IdentityChanged,
    ProcessExited,
    ProtectedTarget,
    PermissionDenied,
    UnsupportedOnPlatform,
    Failed,
}

impl ActionOutcome {
    pub fn is_success(self) -> bool {
        self == ActionOutcome::Ok
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessActionResult {
    pub outcome: ActionOutcome,
    pub pid: u32,
    #[serde(rename = "startTimeSec")]
    pub start_time_sec: u64,
    pub mode: String,
}

#[derive(Debug, Default)]
pub struct ProcessManager {
    system: System,
    warmed_up: bool,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current process table. Sorted by RSS descending for a stable base
    /// order; the UI re-sorts per user preference.
    pub fn page(&mut self, clock: &dyn Clock, logical_cores: usize) -> ProcessPage {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let warmed_up = self.warmed_up;
        self.warmed_up = true;
        let mut rows: Vec<ProcessRow> = self
            .system
            .processes()
            .values()
            .map(|process| {
                let cores_value = if warmed_up {
                    Some(process.cpu_usage().max(0.0) as f64)
                } else {
                    None
                };
                ProcessRow {
                    pid: process.pid().as_u32(),
                    start_time_sec: process.start_time(),
                    name: process.name().to_string_lossy().to_string(),
                    rss_bytes: process.memory(),
                    cpu_usage_cores: cores_value,
                    cpu_usage_host: cores_value.map(|v| {
                        if logical_cores > 0 {
                            v / logical_cores as f64
                        } else {
                            v
                        }
                    }),
                    status: format!("{:?}", process.status()),
                    parent_pid: process.parent().map(|pid| pid.as_u32()),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes).then(a.pid.cmp(&b.pid)));
        ProcessPage {
            collected_at_utc: clock.now_utc_iso8601(),
            cpu_warmup: !warmed_up,
            logical_cores,
            rows,
        }
    }

    /// Attempt a guarded termination. The process identity (PID + start
    /// time) is re-queried immediately before any signal; protected targets
    /// (PID 0/1, our own process) are refused before touching the OS.
    pub fn request_action(&mut self, request: &ProcessActionRequest) -> ProcessActionResult {
        let result = self.attempt_action(request);
        ProcessActionResult {
            outcome: result,
            pid: request.pid,
            start_time_sec: request.start_time_sec,
            mode: request.mode.clone(),
        }
    }

    fn attempt_action(&mut self, request: &ProcessActionRequest) -> ActionOutcome {
        if request.mode != "graceful" && request.mode != "force" {
            return ActionOutcome::Failed;
        }
        if request.pid <= 1 || request.pid == std::process::id() {
            return ActionOutcome::ProtectedTarget;
        }
        let pid = Pid::from_u32(request.pid);
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        let Some(process) = self.system.process(pid) else {
            return ActionOutcome::ProcessExited;
        };
        if process.start_time() != request.start_time_sec {
            // PID reuse: the identity half does not match — refuse.
            return ActionOutcome::IdentityChanged;
        }
        match request.mode.as_str() {
            "graceful" => self.graceful(process),
            _ => {
                if process.kill() {
                    ActionOutcome::Ok
                } else {
                    ActionOutcome::Failed
                }
            }
        }
    }

    #[cfg(unix)]
    fn graceful(&self, process: &sysinfo::Process) -> ActionOutcome {
        match process.kill_with(Signal::Term) {
            Some(true) => ActionOutcome::Ok,
            // Signal sent but reported failed: typically EPERM on a process
            // owned by another user — no elevation, report permission.
            Some(false) => ActionOutcome::PermissionDenied,
            None => ActionOutcome::UnsupportedOnPlatform,
        }
    }

    #[cfg(not(unix))]
    fn graceful(&self, _process: &sysinfo::Process) -> ActionOutcome {
        // Windows: sysinfo exposes no WM_CLOSE-style graceful path; quit is
        // platform_limited until a Win32 adapter lands (documented).
        ActionOutcome::UnsupportedOnPlatform
    }
}

/// Capability row so the Sensors/Capabilities surface lists process data
/// like every other family (FX-021 groundwork).
pub fn capabilities() -> Vec<MetricCapability> {
    vec![MetricCapability {
        metric_id: "process.page".to_string(),
        state: Availability::Available,
        source: COLLECTOR_ID.to_string(),
        explanation: Some(
            "Process metrics are served as a typed page, not part of the metric snapshot."
                .to_string(),
        ),
        reason_code: None,
        remediation: None,
        human_message_key: None,
        precision: Some(1),
        freshness_budget_ms: Some(15_000),
        permissions_required: Vec::new(),
        optional_helper_required: false,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::SystemClock;
    use std::thread;

    fn manager() -> ProcessManager {
        ProcessManager::new()
    }

    #[test]
    fn page_lists_real_processes_with_identity() {
        let mut manager = manager();
        let page = manager.page(&SystemClock, 4);
        assert!(!page.rows.is_empty(), "a booted machine has processes");
        assert!(page.cpu_warmup, "first page is a warm-up");
        // Identity fields present; warm-up rows carry null CPU.
        for row in page.rows.iter().take(5) {
            assert!(row.pid > 0);
            assert!(row.start_time_sec > 0, "start time is the identity half");
            assert_eq!(row.cpu_usage_cores, None);
            assert_eq!(row.cpu_usage_host, None);
            assert!(!row.name.is_empty());
        }
        // Second page: CPU available and within sane bounds.
        thread::sleep(thread_sleep_for_cpu());
        let page2 = manager.page(&SystemClock, 4);
        assert!(!page2.cpu_warmup);
        for row in page2.rows.iter().take(5) {
            let cores = row.cpu_usage_cores.expect("cpu present after warm-up");
            assert!(cores >= 0.0, "core-equivalent must be non-negative");
            let host = row.cpu_usage_host.expect("host cpu present");
            assert!(host <= 100.0 + f64::EPSILON, "host-normalized max 100");
        }
    }

    fn thread_sleep_for_cpu() -> Duration {
        Duration::from_millis(400)
    }

    use std::time::Duration;

    #[test]
    fn protected_targets_are_refused_before_any_os_call() {
        let mut manager = manager();
        let self_pid = std::process::id();
        for pid in [0, 1, self_pid] {
            let result = manager.request_action(&ProcessActionRequest {
                pid,
                start_time_sec: 0,
                mode: "force".into(),
                confirmation_id: "test".into(),
            });
            assert_eq!(
                result.outcome,
                ActionOutcome::ProtectedTarget,
                "pid {pid} must be refused as protected"
            );
        }
    }

    #[test]
    fn unknown_pid_reports_exited() {
        let mut manager = manager();
        // A PID that virtually never exists; start time is irrelevant since
        // the process is absent.
        let result = manager.request_action(&ProcessActionRequest {
            pid: u32::MAX - 7,
            start_time_sec: 1,
            mode: "graceful".into(),
            confirmation_id: "test".into(),
        });
        assert_eq!(result.outcome, ActionOutcome::ProcessExited);
    }

    #[test]
    fn pid_reuse_is_detected_by_start_time_mismatch() {
        let mut manager = manager();
        // Take a real, harmless, still-running process (our own child:
        // this test binary itself has a distinct pid we can observe via
        // the page) and request an action with a WRONG start time. The
        // manager must refuse with IdentityChanged and never signal.
        let page = manager.page(&SystemClock, 4);
        let target = page
            .rows
            .iter()
            .find(|r| r.pid != std::process::id() && r.start_time_sec > 0)
            .expect("at least one other process");
        let result = manager.request_action(&ProcessActionRequest {
            pid: target.pid,
            start_time_sec: target.start_time_sec + 99_999,
            mode: "graceful".into(),
            confirmation_id: "test".into(),
        });
        assert_eq!(
            result.outcome,
            ActionOutcome::IdentityChanged,
            "wrong start time must refuse the action (PID reuse guard)"
        );
    }

    #[test]
    #[ignore = "overhead measurement; run with --ignored --nocapture"]
    fn process_poll_overhead_measurement() {
        // Phase 03 deliverable: measured process-polling overhead (default
        // cadence is slower than core CPU — this is per-call cost).
        let mut manager = manager();
        let _ = manager.page(&SystemClock, 4); // warm-up
        std::thread::sleep(Duration::from_millis(300));
        let started = std::time::Instant::now();
        let page = manager.page(&SystemClock, 4);
        let elapsed = started.elapsed();
        println!(
            "page(): {} rows in {:?} (medium poll group: 10 s cadence)",
            page.rows.len(),
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "page() must stay cheap"
        );
    }

    #[test]
    fn invalid_mode_fails_closed() {
        let mut manager = manager();
        let result = manager.request_action(&ProcessActionRequest {
            pid: 123_456,
            start_time_sec: 1,
            mode: "uninstall".into(),
            confirmation_id: "test".into(),
        });
        assert_eq!(result.outcome, ActionOutcome::Failed);
    }
}
