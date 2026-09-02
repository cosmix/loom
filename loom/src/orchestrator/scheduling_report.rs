//! Why ready stages did not start ("the plan looks stalled — what is it waiting for?").
//!
//! # Why this exists
//!
//! A stage sitting in `Queued` is the single most confusing state loom can
//! present. It means the scheduler considered the stage and declined to start
//! it, but every reason for declining used to be invisible: the spawn guard
//! logged once per daemon run and then went quiet, the concurrency limit
//! logged nothing at all, and `loom status` showed a cheerful "Queued" either
//! way. An operator could not tell "starting in a moment" from "will never
//! start until you intervene".
//!
//! The orchestrator writes this report every poll, replacing it wholesale, so
//! it is always a current snapshot with no stale entries. Both dashboards read
//! it straight from disk rather than asking the daemon — deliberately, since
//! the failure being diagnosed may be a daemon whose scheduler thread has
//! stopped answering. See [`crate::orchestrator::tick`] for that half.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// File name inside `.loom/work/` holding the report.
const REPORT_FILE: &str = "scheduling.json";

/// How long a stage may sit unstarted before it is worth surfacing.
///
/// Below this, "Queued" is just normal scheduling latency — a worktree being
/// created, an auto-merge finishing. Above it, something is holding the stage
/// back and the operator should be told what.
const REPORT_AFTER_SECS: i64 = 60;

/// Why a ready stage was not started this tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockReason {
    /// All session slots are in use. Resolves as running stages finish.
    ConcurrencyLimit { running: usize, max: usize },
    /// The stage is held (`loom stage hold`). Resolves on release.
    Held,
    /// A dependency is not satisfied. `self_resolving` distinguishes "waiting
    /// for the daemon" from "waiting for you".
    Dependency {
        dependency: String,
        detail: String,
        self_resolving: bool,
    },
    /// The dependency check itself failed (git error, unreadable stage file).
    DependencyCheckFailed { detail: String },
    /// Base-branch resolution reported the stage as not schedulable.
    SchedulingNotReady { detail: String },
}

impl BlockReason {
    /// Whether the orchestrator is expected to clear this without help.
    pub fn self_resolving(&self) -> bool {
        match self {
            BlockReason::ConcurrencyLimit { .. } => true,
            BlockReason::Held => false,
            BlockReason::Dependency { self_resolving, .. } => *self_resolving,
            BlockReason::DependencyCheckFailed { .. } => false,
            BlockReason::SchedulingNotReady { .. } => true,
        }
    }

    /// One-line explanation for a dashboard.
    pub fn describe(&self) -> String {
        match self {
            BlockReason::ConcurrencyLimit { running, max } => {
                format!("all {running}/{max} session slots busy")
            }
            BlockReason::Held => "stage is held (`loom stage release <id>`)".to_string(),
            BlockReason::Dependency {
                dependency, detail, ..
            } => format!("dependency '{dependency}' {detail}"),
            BlockReason::DependencyCheckFailed { detail } => {
                format!("dependency check failed: {detail}")
            }
            BlockReason::SchedulingNotReady { detail } => detail.clone(),
        }
    }
}

/// A ready-but-unstarted stage and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedStage {
    pub stage_id: String,
    /// When the orchestrator first saw this stage ready and could not start
    /// it. In-memory on the daemon side, so it resets on restart — which is
    /// correct: a restart is a fresh scheduling attempt.
    pub queued_since: DateTime<Utc>,
    pub reason: BlockReason,
}

impl BlockedStage {
    pub fn waiting_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.queued_since).num_seconds().max(0)
    }
}

/// The full snapshot written each poll.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingReport {
    pub blocked: Vec<BlockedStage>,
}

fn report_path(work_dir: &Path) -> PathBuf {
    work_dir.join(REPORT_FILE)
}

/// Write the snapshot, replacing any previous one.
///
/// Best-effort, like the tick: a diagnostic that cannot be written must not
/// disturb scheduling.
pub fn write(work_dir: &Path, report: &SchedulingReport) {
    if let Ok(json) = serde_json::to_string_pretty(report) {
        let _ = std::fs::write(report_path(work_dir), json);
    }
}

/// Read the last snapshot. `Ok(None)` when absent or unparseable.
pub fn read(work_dir: &Path) -> Result<Option<SchedulingReport>> {
    let path = report_path(work_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content).ok())
}

/// Remove the report (daemon shutdown).
pub fn clear(work_dir: &Path) {
    let _ = std::fs::remove_file(report_path(work_dir));
}

/// How loudly to render an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Progress is happening, but slower than expected, or waiting on a slot.
    Info,
    /// Needs a human; nothing will change on its own.
    Warning,
    /// The scheduler itself has stopped.
    Critical,
}

/// A display-ready line for either dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub severity: Severity,
    pub text: String,
}

/// Build the alert list shown by `loom status` and the live TUI.
///
/// Both dashboards call this so their wording and thresholds cannot drift
/// apart.
///
/// Everything here is gated on `daemon_running`. Both files describe live
/// scheduling, and a daemon killed with SIGKILL leaves both behind: without
/// the gate, a stopped daemon would report its final tick as a stall and its
/// last blocked stages as though they were still waiting.
pub fn alerts(work_dir: &Path, daemon_running: bool) -> Vec<Alert> {
    let mut alerts = Vec::new();
    if !daemon_running {
        return alerts;
    }

    let now = Utc::now();

    if let Some(alert) = stalled_alert(work_dir, now) {
        alerts.push(alert);
    }

    let Ok(Some(report)) = read(work_dir) else {
        return alerts;
    };

    for blocked in &report.blocked {
        let waited = blocked.waiting_secs(now);
        if waited < REPORT_AFTER_SECS {
            continue;
        }

        alerts.push(Alert {
            severity: if blocked.reason.self_resolving() {
                Severity::Info
            } else {
                Severity::Warning
            },
            text: format!(
                "{} queued {} — {}",
                blocked.stage_id,
                crate::utils::format_elapsed(waited),
                blocked.reason.describe()
            ),
        });
    }

    alerts
}

fn stalled_alert(work_dir: &Path, now: DateTime<Utc>) -> Option<Alert> {
    let tick = crate::orchestrator::tick::read(work_dir).ok().flatten()?;
    if !tick.is_stalled(now) {
        return None;
    }

    let phase = tick
        .phase
        .map(|p| format!(", stuck in {}", p.as_str()))
        .unwrap_or_default();
    Some(Alert {
        severity: Severity::Critical,
        text: format!(
            "orchestrator loop stalled: no tick for {}s{} — restart with \
             `loom stop`, then `loom run`",
            tick.age_secs(now),
            phase
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    fn blocked_for(secs: i64, reason: BlockReason) -> BlockedStage {
        BlockedStage {
            stage_id: "web-features".to_string(),
            queued_since: Utc::now() - Duration::seconds(secs),
            reason,
        }
    }

    #[test]
    fn test_roundtrip_preserves_reason() {
        let temp = TempDir::new().unwrap();
        let report = SchedulingReport {
            blocked: vec![blocked_for(
                120,
                BlockReason::Dependency {
                    dependency: "backend-features".to_string(),
                    detail: "Completed but not merged yet".to_string(),
                    self_resolving: true,
                },
            )],
        };

        write(temp.path(), &report);
        let loaded = read(temp.path()).unwrap().expect("report should exist");

        assert_eq!(loaded, report);
    }

    #[test]
    fn test_recently_queued_stage_is_not_reported() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            &SchedulingReport {
                blocked: vec![blocked_for(5, BlockReason::Held)],
            },
        );

        // Normal scheduling latency must not produce alert noise.
        assert!(alerts(temp.path(), true).is_empty());
    }

    #[test]
    fn test_long_queued_stage_reports_reason_and_duration() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            &SchedulingReport {
                blocked: vec![blocked_for(
                    600,
                    BlockReason::Dependency {
                        dependency: "backend-features".to_string(),
                        detail: "Completed but not merged yet".to_string(),
                        self_resolving: true,
                    },
                )],
            },
        );

        let alerts = alerts(temp.path(), true);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, Severity::Info);
        assert!(alerts[0].text.contains("web-features queued"));
        assert!(alerts[0].text.contains("backend-features"));
        assert!(alerts[0].text.contains("not merged"));
    }

    #[test]
    fn test_non_self_resolving_block_is_a_warning() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            &SchedulingReport {
                blocked: vec![blocked_for(
                    600,
                    BlockReason::Dependency {
                        dependency: "backend-core".to_string(),
                        detail: "Skipped — dependents can never become ready".to_string(),
                        self_resolving: false,
                    },
                )],
            },
        );

        assert_eq!(alerts(temp.path(), true)[0].severity, Severity::Warning);
    }

    #[test]
    fn test_concurrency_limit_is_self_resolving() {
        assert!(BlockReason::ConcurrencyLimit { running: 4, max: 4 }.self_resolving());
        assert!(!BlockReason::Held.self_resolving());
    }

    #[test]
    fn test_stall_alert_requires_a_running_daemon() {
        let temp = TempDir::new().unwrap();
        crate::orchestrator::tick::record(temp.path(), crate::orchestrator::tick::Phase::Events);

        // Backdate the tick well past the stall threshold.
        std::fs::write(
            temp.path().join("orchestrator.tick"),
            format!(
                "{}\nevents\n",
                (Utc::now() - Duration::hours(10)).to_rfc3339()
            ),
        )
        .unwrap();

        assert!(alerts(temp.path(), false).is_empty(), "stopped daemon");

        let running = alerts(temp.path(), true);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].severity, Severity::Critical);
        assert!(running[0].text.contains("stuck in events"));
    }

    #[test]
    fn test_stopped_daemon_suppresses_stale_block_alerts() {
        // A SIGKILLed daemon cannot run its shutdown cleanup, so the report
        // outlives it. Those entries describe scheduling that is no longer
        // happening and must not render as live warnings.
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            &SchedulingReport {
                blocked: vec![blocked_for(600, BlockReason::Held)],
            },
        );

        assert!(alerts(temp.path(), false).is_empty());
        assert_eq!(alerts(temp.path(), true).len(), 1);
    }

    #[test]
    fn test_missing_report_yields_no_alerts() {
        let temp = TempDir::new().unwrap();
        assert!(alerts(temp.path(), true).is_empty());
    }
}
