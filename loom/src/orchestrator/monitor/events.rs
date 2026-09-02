//! Events detected by the monitor

use std::path::PathBuf;

/// Events detected by the monitor
#[derive(Debug, Clone, PartialEq)]
pub enum MonitorEvent {
    StageCompleted {
        stage_id: String,
    },
    StageBlocked {
        stage_id: String,
        reason: String,
    },
    SessionContextWarning {
        session_id: String,
        context_tokens: u32,
        ceiling_tokens: u32,
    },
    SessionContextCritical {
        session_id: String,
        context_tokens: u32,
        ceiling_tokens: u32,
    },
    SessionCrashed {
        session_id: String,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    },
    /// Session is hung (PID alive but no heartbeat for its response budget)
    SessionHung {
        session_id: String,
        stage_id: Option<String>,
        /// How long since last heartbeat in seconds
        stale_duration_secs: u64,
        /// The response budget that was exceeded, in seconds. Per-stage
        /// (`subagent_timeout_secs`) or the built-in default — reported so the
        /// warning names the threshold it was measured against.
        timeout_secs: u64,
        /// Last known activity from heartbeat
        last_activity: Option<String>,
        /// The stage looks FINISHED rather than stuck: its branch carries
        /// commits beyond its base and its worktree is clean, so the session
        /// most likely ended its turn without running `loom stage complete`.
        /// Advisory like the rest of this event — it only sharpens the warning.
        finished_without_completing: bool,
    },
    /// An adjudication session is alive but has stopped working: no tool call
    /// for longer than the disputed stage's response budget.
    ///
    /// Separate from [`MonitorEvent::SessionHung`] because the remedy is
    /// different. A stalled stage agent's stage is handed off and re-queued; a
    /// stalled judge is simply closed, leaving its stage in
    /// `NeedsAdjudication` for the next poll to re-judge under the dispute's
    /// own attempt budget.
    AdjudicatorStalled {
        session_id: String,
        /// The disputed stage the judge was spawned for.
        stage_id: String,
        /// Time since the judge's last tool call, or since it was spawned if
        /// it has never made one.
        stale_duration_secs: u64,
        /// The stage response budget that was exceeded, in seconds.
        timeout_secs: u64,
    },
    SessionNeedsHandoff {
        session_id: String,
        stage_id: String,
    },
    /// Stage is waiting for user input
    StageWaitingForInput {
        stage_id: String,
        session_id: Option<String>,
    },
    /// Stage resumed execution after user input
    StageResumedExecution {
        stage_id: String,
    },
    /// Merge session completed (conflict resolution session finished)
    MergeSessionCompleted {
        session_id: String,
        stage_id: String,
    },
    /// Heartbeat received from a session
    HeartbeatReceived {
        stage_id: String,
        session_id: String,
        /// Resident tokens, or `None` when the hook could not measure them.
        context_tokens: Option<u32>,
        transcript_path: Option<String>,
        last_tool: Option<String>,
    },
    /// A session ran past the daemon's backstop multiple of its stage ceiling -
    /// forced handoff required. `ceiling_tokens` is the stage's own ceiling;
    /// the backstop fires at `DAEMON_CEILING_MULTIPLIER` times that.
    BudgetExceeded {
        session_id: String,
        stage_id: String,
        context_tokens: u32,
        ceiling_tokens: u32,
    },
    /// Stage needs human review - agent flagged something for human judgment
    StageNeedsHumanReview {
        stage_id: String,
        review_reason: Option<String>,
    },
}
