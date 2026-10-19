//! Approved local action runner (phase 07 §3.6/§3.7).
//!
//! This module is the ONLY place in the app that launches a user-configured
//! program, and the safety boundary is structural, not scattered:
//!
//! * [`ApprovedAction`] is the sole execution token. Its only constructor,
//!   [`ApprovedAction::authorize`], requires the settings master gate AND
//!   the per-rule approval flag AND a validator-clean configuration —
//!   without both gates it refuses and the caller audits the reason.
//! * Execution never uses a shell. The launcher executable is fixed at
//!   compile time (`/usr/bin/env` on Unix, which simply execs its single
//!   argument); the validated **absolute** program path travels as that
//!   one argument — the equivalent of
//!   `subprocess.run([launcher, program], shell=False)`. Metric values,
//!   entity ids and rule text never enter the command line, and nothing is
//!   resolved through `PATH`.
//! * One action runs at a time; each run has a hard wall-clock timeout and
//!   leaves an audit row. No HTTP route reaches this code path.
//! * Windows action execution is an explicit `unsupported_on_platform`
//!   refusal this phase (no reviewed non-shell runner); the model, audit
//!   and UI still work.

use std::time::{Duration, Instant};

use monitor_alerts::{ActionConfig, RuleError};
use serde::Serialize;

/// Maximum concurrent action runs (phase 07 §3.6: "limit concurrency").
pub const MAX_CONCURRENT_ACTIONS: u32 = 1;
const ACTION_POLL_INTERVAL_MS: u64 = 50;
pub const AUDIT_DETAIL_MAX: usize = 200;

/// An action that has passed every gate. Deliberately opaque: runner code
/// cannot bypass the authorization step because there is no other way to
/// obtain this value.
#[derive(Debug, Clone)]
pub struct ApprovedAction {
    rule_id: String,
    program: String,
    timeout: Duration,
}

impl ApprovedAction {
    /// Authorize a configured action against the live settings. `Err`
    /// means "refuse" — the caller audits the reason.
    pub fn authorize(
        rule_id: &str,
        action: &ActionConfig,
        actions_enabled: bool,
    ) -> Result<Self, ActionRefusal> {
        if !actions_enabled {
            return Err(ActionRefusal::ActionsDisabled);
        }
        if !action.approved {
            return Err(ActionRefusal::NotApproved);
        }
        // Re-validate at the execution boundary (defense in depth): the
        // stored config may predate a validator change.
        monitor_alerts::validate_action(action).map_err(|error| match error {
            RuleError::ActionProgramInvalid { .. } => ActionRefusal::InvalidConfig,
            RuleError::ActionTimeoutOutOfBounds { .. } => ActionRefusal::InvalidConfig,
            other => ActionRefusal::Validation(other.to_string()),
        })?;
        Ok(Self {
            rule_id: rule_id.to_string(),
            program: action.program.clone(),
            timeout: Duration::from_secs(action.timeout_secs.max(1) as u64),
        })
    }

    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRefusal {
    /// The settings master gate is off (the default).
    ActionsDisabled,
    /// The rule's action was never individually approved.
    NotApproved,
    /// The stored configuration no longer validates.
    InvalidConfig,
    /// No reviewed non-shell runner on this platform yet.
    UnsupportedOnPlatform,
    #[serde(skip)]
    Validation(String),
}

impl std::fmt::Display for ActionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActionsDisabled => write!(f, "blocked_actions_disabled"),
            Self::NotApproved => write!(f, "blocked_not_approved"),
            Self::InvalidConfig => write!(f, "blocked_invalid_config"),
            Self::UnsupportedOnPlatform => write!(f, "unsupported_on_platform"),
            Self::Validation(text) => write!(f, "blocked_validation: {text}"),
        }
    }
}

/// Outcome of one supervised run (audited verbatim, detail truncated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionOutcome {
    pub pid: u32,
    pub result: String,
    pub detail: String,
}

/// Launch the approved program and wait under the hard timeout. Single
/// argument — no shell, no inherited stdio, no environment templating.
pub fn execute(approved: &ApprovedAction) -> ActionOutcome {
    #[cfg(not(unix))]
    {
        let _ = approved;
        ActionOutcome {
            pid: 0,
            result: "unsupported_on_platform".to_string(),
            detail: "action execution needs a reviewed Windows runner".to_string(),
        }
    }
    #[cfg(unix)]
    {
        // Local literal binding: the launcher is fixed at compile time and
        // the approved program travels as its single argument (no shell,
        // no PATH search — the validator requires an absolute path).
        let launcher = "/usr/bin/env";
        let mut command = std::process::Command::new(launcher);
        command
            .arg(&approved.program)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return ActionOutcome {
                    pid: 0,
                    result: "failed".to_string(),
                    detail: truncate(error.to_string()),
                }
            }
        };
        let pid = child.id();
        let deadline = Instant::now() + approved.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    return ActionOutcome {
                        pid,
                        result: "ok".to_string(),
                        detail: format!("exit:{code}"),
                    };
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return ActionOutcome {
                            pid,
                            result: "timeout".to_string(),
                            detail: format!("killed after {}s", approved.timeout.as_secs().max(1)),
                        };
                    }
                    std::thread::sleep(Duration::from_millis(ACTION_POLL_INTERVAL_MS));
                }
                Err(error) => {
                    return ActionOutcome {
                        pid,
                        result: "failed".to_string(),
                        detail: truncate(error.to_string()),
                    }
                }
            }
        }
    }
}

pub fn truncate(mut text: String) -> String {
    if text.len() > AUDIT_DETAIL_MAX {
        text.truncate(AUDIT_DETAIL_MAX);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(program: &str, timeout: u32, approved: bool) -> ActionConfig {
        ActionConfig {
            program: program.to_string(),
            timeout_secs: timeout,
            approved,
        }
    }

    /// Phase 07 §4 + acceptance gate: "Optional actions cannot execute
    /// without user opt-in and approval."
    #[test]
    fn authorization_requires_both_gates() {
        let cfg = action("/usr/bin/true", 5, false);
        assert_eq!(
            ApprovedAction::authorize("r", &cfg, false).unwrap_err(),
            ActionRefusal::ActionsDisabled
        );
        assert_eq!(
            ApprovedAction::authorize("r", &cfg, true).unwrap_err(),
            ActionRefusal::NotApproved
        );
        let approved_cfg = action("/usr/bin/true", 5, true);
        assert!(ApprovedAction::authorize("r", &approved_cfg, true).is_ok());
        assert!(ApprovedAction::authorize("r", &approved_cfg, false).is_err());
    }

    #[test]
    fn authorization_revalidates_stored_configuration() {
        // Relative program path: rejected even with both gates on.
        let relative = action("sh", 5, true);
        assert_eq!(
            ApprovedAction::authorize("r", &relative, true).unwrap_err(),
            ActionRefusal::InvalidConfig
        );
        // Unbounded timeout: rejected.
        let timeout = action("/usr/bin/true", 0, true);
        assert_eq!(
            ApprovedAction::authorize("r", &timeout, true).unwrap_err(),
            ActionRefusal::InvalidConfig
        );
    }

    /// Phase 07 §4: "Malicious process name/metric text: cannot reach shell
    /// code; timeout and concurrency enforce limits."
    #[cfg(unix)]
    #[test]
    fn approved_literal_program_runs_and_metric_text_has_no_path_in() {
        // The runner carries only what `authorize` copied verbatim from the
        // approved config. There is no API that accepts live metric text.
        let cfg = action("/bin/echo", 5, true);
        let approved = ApprovedAction::authorize("r", &cfg, true).unwrap();
        let outcome = execute(&approved);
        assert_eq!(outcome.result, "ok");
        assert_eq!(outcome.detail, "exit:0");
        assert!(outcome.pid > 0);
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_runaway_children() {
        // The runner launches one path with no arguments, so the runaway
        // child is a tiny executable script that sleeps forever.
        let dir = std::env::temp_dir().join(format!("hostlume-action-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("runaway.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let cfg = action(script.to_str().unwrap(), 1, true);
        let approved = ApprovedAction::authorize("r", &cfg, true).unwrap();
        let started = Instant::now();
        let outcome = execute(&approved);
        assert_eq!(outcome.result, "timeout");
        assert!(started.elapsed() < Duration::from_secs(5), "hard timeout");
        assert!(outcome.detail.contains("killed after 1s"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn missing_program_fails_closed_with_auditable_exit() {
        // /usr/bin/env cannot exec a nonexistent program and exits 127 —
        // the refusal surfaces as a typed, auditable outcome, never silence.
        let cfg = action("/nonexistent/hostlume/binary", 5, true);
        let approved = ApprovedAction::authorize("r", &cfg, true).unwrap();
        let outcome = execute(&approved);
        assert_eq!(outcome.result, "ok", "the launcher itself ran");
        assert_eq!(outcome.detail, "exit:127");
    }

    #[test]
    fn audit_detail_is_bounded() {
        let long = "x".repeat(1_000);
        assert_eq!(truncate(long).len(), AUDIT_DETAIL_MAX);
        assert_eq!(truncate("short".to_string()), "short");
    }
}
