mod busy;
mod collector;
pub(crate) mod completion_view;
mod criteria_results;
mod disputes;
mod execution_models;
mod heartbeat_facts;
mod merge_note;
mod resolver;
mod sanitize;
mod stopped;
mod timing;

pub use collector::{collect_status_data, load_all_sessions};
pub use completion_view::{
    completion_blocker_summary, outgoing_exit_reason, CompletionBlockerState,
    CompletionBlockerSummary,
};
pub use criteria_results::FailingCriterion;
pub use disputes::{DisputeLine, DisputeOp};
pub use execution_models::execution_models_for_stage;

use serde::{Deserialize, Serialize};

// Re-export types that consumers will need
pub use crate::models::failure::FailureInfo;
pub use crate::models::session::SessionExitReason;
pub use crate::models::session::{SessionBackendKind, SessionType};
pub use crate::models::stage::{StageStatus, StageType};

/// Activity status derived from heartbeat and session state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ActivityStatus {
    /// No active session, the session ended without crashing, or the stage
    /// is finished (Completed/Skipped).
    #[default]
    Idle,
    /// Session is actively working (recent heartbeat)
    Working,
    /// Session encountered an error or crashed
    Error,
    /// Session may be hung: its heartbeat has been silent past the stage's
    /// `subagent_timeout_secs` budget (the threshold the monitor acts on) and
    /// no tool call is open.
    Stale,
    /// A tool call is open (the main agent's or a subagent's transcript ends
    /// in a `tool_use` with no result), so a silent heartbeat is expected. The
    /// silence is still reported so a stuck command stays visible.
    Busy,
    /// Heartbeat silent past the default staleness threshold but within the
    /// stage's larger budget, with no tool call open. Neutral, not a warning.
    Quiet,
    /// Stage status claims Executing but no session record exists for it —
    /// e.g. the daemon was killed and restarted while the file that would
    /// have named the running agent's session was never written or was lost.
    /// Distinct from `Idle`: there is nothing quiet here, the tracking data
    /// itself is missing.
    Orphaned,
}

impl ActivityStatus {
    /// Get Unicode icon for this activity status
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "\u{23F3}",      // hourglass
            Self::Working => "\u{1F504}",  // arrows counterclockwise
            Self::Error => "\u{274C}",     // cross mark
            Self::Stale => "\u{26A0}",     // warning
            Self::Busy => "\u{2699}",      // gear
            Self::Quiet => "\u{1F4A4}",    // zzz
            Self::Orphaned => "\u{1F480}", // skull
        }
    }

    /// Get a short label for this status
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Working => "WORKING",
            Self::Error => "ERROR",
            Self::Stale => "STALE",
            Self::Busy => "BUSY",
            Self::Quiet => "QUIET",
            Self::Orphaned => "ORPHANED",
        }
    }
}

/// Main struct aggregating all displayable status information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusData {
    pub stages: Vec<StageSummary>,
    pub merge: MergeSummary,
    pub progress: ProgressSummary,
    /// Extracted plan name (first H1 header from the plan file)
    pub plan_name: Option<String>,
    /// Cached provider quota usage, read independently of the poll that
    /// populates it - always present on the wire, `None` per-provider when
    /// no cache entry exists yet.
    #[serde(default)]
    pub quota: crate::quota::QuotaSnapshot,
    /// Live token flow from the daemon's tracker; absent when no fresh
    /// `flow.json` exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<crate::tokens::TokenFlow>,
    /// The most recent stage status changes, oldest first, from the durable
    /// event log.
    #[serde(default)]
    pub activity: Vec<crate::fs::stage_events::StageEvent>,
}

/// Stage display data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    /// Plan-authored description of what the stage does, shown in the stage
    /// detail card below its heading. `None` when the plan stage has none.
    #[serde(default)]
    pub description: Option<String>,
    /// Plan-authored one-to-three sentence summary for humans, shown by the
    /// web dashboard instead of `description`. `None` when the plan has none.
    #[serde(default)]
    pub summary: Option<String>,
    pub status: StageStatus,
    /// Type of stage (standard, knowledge, integration-verify)
    #[serde(default)]
    pub stage_type: StageType,
    pub dependencies: Vec<String>,
    /// Resident context tokens for the active session.
    pub context_tokens: Option<u32>,
    /// Wall-clock life of the stage: from its first start to completion, or to
    /// now while still open. `None` until the stage has started.
    pub elapsed_secs: Option<i64>,
    /// Accumulated execution time (excludes wait/backoff time). For an
    /// `Executing` stage this includes the attempt currently in flight, not
    /// just the time banked by prior attempts.
    pub execution_secs: Option<i64>,
    pub base_branch: Option<String>,
    pub base_merged_from: Vec<String>,
    pub failure_info: Option<FailureInfo>,
    /// Activity status derived from heartbeat
    pub activity_status: ActivityStatus,
    /// Last tool used (from heartbeat); while `activity_status` is `Busy`,
    /// the tool call that is open instead, `None` when its name is unknown
    pub last_tool: Option<String>,
    /// Human-readable activity description
    pub last_activity: Option<String>,
    /// Seconds since last heartbeat (for staleness detection)
    pub staleness_secs: Option<u64>,
    /// Resolved context ceiling for the active session.
    pub context_ceiling_tokens: Option<u32>,
    /// Reason the stage was flagged for human review
    pub review_reason: Option<String>,
    /// Complete multi-line review reason for the web dashboard; `review_reason`
    /// is the flattened, bounded line the terminal renderers print.
    #[serde(default)]
    pub review_notes: Option<String>,
    /// Whether stage changes have been merged to the merge point
    pub merged: bool,
    /// `merged` was asserted by `--assume-merged`, not set by a merge loom performed.
    #[serde(default)]
    pub merge_assumed: bool,
    /// Why the post-merge cleanup failed, if it did (worktree/branch still on disk)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_warning: Option<String>,
    /// The typed reason the merge is blocked (the block's sentence), while the
    /// stage is `MergeBlocked`; the daemon retries it every tick
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_block: Option<String>,
    /// Backup ref holding changes a merge stashed in the main checkout and
    /// could not restore, while that ref still exists
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stash_warning: Option<String>,
    /// Whether the stage is held
    pub held: bool,
    /// Current retry count
    pub retry_count: u32,
    /// Maximum retries allowed
    pub max_retries: Option<u32>,
    /// Session PID (if executing)
    pub pid: Option<u32>,
    /// Whether the session process is alive
    pub session_alive: bool,
    /// Effective model name for this stage (explicit override or stage-type default)
    pub model: String,
    /// The kind of the session that speaks for the stage, if any: the one
    /// `stage.session` names, or the live merge resolver once the stage is in
    /// a merge state and its worker has ended. An `Executing` stage whose
    /// session is not of its own worker kind (e.g. an adjudication session)
    /// is not describing a working agent — see `incoherence`.
    pub session_type: Option<SessionType>,
    /// Why an `Executing` stage does not describe a working agent, if it
    /// does not. `None` for every other stage.
    pub incoherence: Option<String>,
    /// Distinct execution-model display names observed for this stage's subagents,
    /// in first-seen order (spawn ledger, then codex ledger). Empty until a
    /// subagent spawns.
    #[serde(default)]
    pub execution_models: Vec<String>,
    /// Disputes filed against this stage's acceptance criteria.
    #[serde(default)]
    pub dispute_count: u32,
    /// Age in seconds of the adjudication session's heartbeat for this stage.
    /// `None` when no judge has written one.
    #[serde(default)]
    pub judge_heartbeat_secs: Option<u64>,
    /// Which terminal backend hosts this stage's session, when one is known.
    #[serde(default)]
    pub session_backend: Option<crate::models::session::SessionBackendKind>,
    /// Why the session named by `stage.session` exited, once terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outgoing_session_exit_reason: Option<SessionExitReason>,
    /// Actionable completion failure associated with the exact current session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_blocker: Option<CompletionBlockerSummary>,
    /// Id of the live merge-resolver session for a `MergeConflict` or
    /// `MergeBlocked` stage. Feeds the attention note and the dashboard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_resolver_session: Option<String>,
    /// Merge-resolver sessions the daemon has spawned for a `MergeConflict` or
    /// `MergeBlocked` stage; `None` for every other stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_resolver_attempts: Option<u32>,
    /// Why the stage was blocked (`stage.close_reason`). Feeds the attention
    /// note for an agent's `loom stage block`. `None` for every other status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    /// The commands of a blocked stage's acceptance criteria, in the order
    /// `loom stage amend --index` counts them from 0. Empty for every other
    /// stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance: Vec<String>,
    /// Id of the newest ended tmux session of this stage that left a terminal
    /// history file (`terminal-history/<id>.terminal`). The dashboard opens it read-only
    /// once the stage has no live terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_history_session: Option<String>,
    /// What the attention card for a blocked stage shows beyond the reason.
    #[serde(default, skip_serializing_if = "BlockDetail::is_empty")]
    pub block_detail: BlockDetail,
}

/// The facts about a blocked stage that the attention card lists: which
/// criteria failed and which disputes were already ruled on. Empty for every
/// other stage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDetail {
    /// Acceptance criteria that failed in the stage's last completion attempt.
    #[serde(default)]
    pub failing_criteria: Vec<FailingCriterion>,
    /// The stage's newest disputes, oldest first.
    #[serde(default)]
    pub disputes: Vec<DisputeLine>,
}

impl BlockDetail {
    pub fn is_empty(&self) -> bool {
        self.failing_criteria.is_empty() && self.disputes.is_empty()
    }
}

impl StageSummary {
    /// The word a merged stage wears: `assumed` when an operator asserted the
    /// merge with `--assume-merged`, `merged` when loom performed it.
    pub fn merge_label(&self) -> &'static str {
        if self.merge_assumed {
            "assumed"
        } else {
            "merged"
        }
    }
}

/// Session display data (test-only)
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub stage_id: Option<String>,
    pub pid: Option<u32>,
    pub context_tokens: u32,
    pub uptime_secs: i64,
    pub is_alive: bool,
}

/// Merge state summary
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MergeSummary {
    pub merged: Vec<String>,
    pub pending: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Progress counts
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProgressSummary {
    pub total: usize,
    pub completed: usize,
    pub executing: usize,
    pub pending: usize,
    pub blocked: usize,
}
