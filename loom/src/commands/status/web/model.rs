use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::commands::status::data::{DisputeLine, FailingCriterion, StatusData};
use crate::commands::status::render::attention_model::{Alternative, AttentionEntry};
use crate::commands::status::render::failure_label;
use crate::daemon::{DaemonServer, DaemonStatus};
use crate::models::failure::FailureType;
use crate::orchestrator::scheduling_report::{self, Alert, Severity};
use crate::orchestrator::tick;

/// One frame of the web dashboard: everything the page renders, as one JSON object.
/// Served by `/api/status` and pushed as every WebSocket text frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSnapshot {
    /// The exact status payload exposed by the live CLI.
    pub status: StatusData,
    /// Stages requiring operator attention, in stage order.
    pub attention: Vec<WebAttention>,
    /// Scheduler alerts shared with the live CLI.
    pub alerts: Vec<WebAlert>,
    /// Current daemon process and socket state.
    pub daemon: DaemonState,
    /// Age of the most recent orchestrator tick.
    pub tick_age_secs: Option<i64>,
    /// Whether this frame came from the daemon or local files.
    pub source: SnapshotSource,
    /// Whether this server accepts browser terminal attachments (`loom status --web --terminals`).
    pub terminals: bool,
    /// An explanation shown when the daemon lane is degraded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// Time at which this frame was generated.
    pub generated_at: DateTime<Utc>,
    /// The version of the loom build serving the dashboard.
    pub version: String,
}

/// The daemon state represented on the dashboard wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DaemonState {
    /// The daemon process and socket are responsive.
    Running,
    /// The daemon process exists but its socket is not responsive.
    ProcessOnly,
    /// No daemon owns the work directory.
    NotRunning,
    /// The daemon owns the work directory but this caller cannot use its socket.
    Unreachable,
}

/// The source used to produce a dashboard frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotSource {
    /// Data received from the daemon subscription.
    Daemon,
    /// Data collected directly from the work directory.
    Files,
}

/// An [`AttentionEntry`] with owned strings so it can be serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAttention {
    /// Stable stage identifier.
    pub id: String,
    /// Human-readable stage name.
    pub name: String,
    /// Short attention label.
    pub label: String,
    /// Shell command the operator should run; `None` when no single command
    /// applies or loom is handling the state.
    pub command: Option<String>,
    /// When to run `command`, phrased to follow "Once "; `None` when it is the
    /// first thing to do.
    pub command_when: Option<String>,
    /// Other commands for other situations, in the order to offer them.
    pub alternatives: Vec<Alternative>,
    /// What loom is doing, or what the operator should do when no single
    /// command fits. Prose, never a command meant for copying.
    pub note: Option<String>,
    /// Whether loom is handling this state itself, so the operator need not act.
    pub automatic: bool,
    /// Failure category, when present.
    pub failure_type: Option<FailureType>,
    /// Short label for the failure category.
    pub failure_label: Option<String>,
    /// Evidence collected for the failure.
    pub evidence: Vec<String>,
    /// Human review explanation, when present.
    pub review_reason: Option<String>,
    /// Why the stage needs the operator now: a block's close reason or a
    /// review's reason. Shown in the card header; `None` when neither applies.
    pub reason: Option<String>,
    /// The acceptance commands a blocked stage offers to drop or replace, in
    /// `loom stage amend --index` order (from 0).
    pub acceptance: Vec<String>,
    /// Acceptance criteria that failed in the stage's last completion attempt.
    pub failing_criteria: Vec<FailingCriterion>,
    /// Disputes already ruled on for a blocked stage, oldest first.
    pub disputes: Vec<DisputeLine>,
    /// Whether the stage left a terminal history the dashboard can open.
    pub terminal_history: bool,
    /// Post-merge cleanup warning, when present.
    pub cleanup_warning: Option<String>,
    /// Whether the attention item has human review choices.
    pub has_human_review_choices: bool,
    /// Number of disputes, when adjudicating.
    pub dispute_count: Option<u32>,
    /// Age of the judge heartbeat, when adjudicating.
    pub judge_heartbeat_secs: Option<u64>,
}

/// A scheduler alert rendered by the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAlert {
    /// Alert severity.
    pub severity: WebSeverity,
    /// Human-readable alert text.
    pub text: String,
}

/// Severity levels used by dashboard alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSeverity {
    /// Informational scheduler activity.
    Info,
    /// An operator may need to intervene.
    Warning,
    /// The scheduler itself appears stalled.
    Critical,
}

impl From<&AttentionEntry> for WebAttention {
    fn from(entry: &AttentionEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            label: entry.label.to_owned(),
            command: entry.command.clone(),
            command_when: entry.command_when.clone(),
            alternatives: entry.alternatives.clone(),
            note: entry.note.clone(),
            automatic: entry.automatic,
            failure_type: entry.failure_type.clone(),
            failure_label: entry
                .failure_type
                .as_ref()
                .map(failure_label)
                .map(str::to_owned),
            evidence: entry.evidence.clone(),
            review_reason: entry.review_reason.clone(),
            reason: entry.reason.clone(),
            acceptance: entry.acceptance.clone(),
            failing_criteria: entry.failing_criteria.clone(),
            disputes: entry.disputes.clone(),
            terminal_history: entry.terminal_history.is_some(),
            cleanup_warning: entry.cleanup_warning.clone(),
            has_human_review_choices: entry.has_human_review_choices,
            dispute_count: entry.dispute_count,
            judge_heartbeat_secs: entry.judge_heartbeat_secs,
        }
    }
}

impl From<&Alert> for WebAlert {
    fn from(alert: &Alert) -> Self {
        let severity = match alert.severity {
            Severity::Info => WebSeverity::Info,
            Severity::Warning => WebSeverity::Warning,
            Severity::Critical => WebSeverity::Critical,
        };
        Self {
            severity,
            text: alert.text.clone(),
        }
    }
}

impl From<DaemonStatus> for DaemonState {
    fn from(status: DaemonStatus) -> Self {
        match status {
            DaemonStatus::Running => Self::Running,
            DaemonStatus::ProcessOnly => Self::ProcessOnly,
            DaemonStatus::NotRunning => Self::NotRunning,
            DaemonStatus::Unreachable => Self::Unreachable,
        }
    }
}

impl DaemonState {
    /// Whether scheduler alerts should treat the daemon as running.
    /// `Unreachable` means this process's sandbox cannot open the socket.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Unreachable)
    }
}

/// Wrap a [`StatusData`] with everything the TUI otherwise computes client-side.
/// `work_path` is the `.loom/work` directory (`WorkDir::root()`).
pub fn collect_snapshot(
    work_path: &Path,
    status: StatusData,
    source: SnapshotSource,
    terminals: bool,
) -> WebSnapshot {
    let daemon = DaemonState::from(DaemonServer::check_status(work_path));
    let attention = crate::commands::status::render::attention_entries(&status.stages)
        .iter()
        .map(WebAttention::from)
        .collect();
    let alerts = scheduling_report::alerts(work_path, daemon.is_running())
        .iter()
        .map(WebAlert::from)
        .collect();
    let tick_age_secs = tick::read(work_path)
        .ok()
        .flatten()
        .map(|tick| tick.age_secs(Utc::now()));
    WebSnapshot {
        status,
        attention,
        alerts,
        daemon,
        tick_age_secs,
        source,
        terminals,
        notice: None,
        generated_at: Utc::now(),
        version: crate::version::VERSION.to_owned(),
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
