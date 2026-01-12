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
        usage_percent: f32,
    },
    SessionContextCritical {
        session_id: String,
        usage_percent: f32,
    },
    SessionCrashed {
        session_id: String,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    },
    /// Session is hung (PID alive but no heartbeat for extended period)
    SessionHung {
        session_id: String,
        stage_id: Option<String>,
        /// How long since last heartbeat in seconds
        stale_duration_secs: u64,
        /// Last known activity from heartbeat
        last_activity: Option<String>,
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
    /// Checkpoint created for a task
    CheckpointCreated {
        session_id: String,
        task_id: String,
        verification_passed: bool,
        warnings: Vec<String>,
        stage_complete: bool,
    },
    /// Heartbeat received from a session
    HeartbeatReceived {
        stage_id: String,
        session_id: String,
        context_percent: Option<f32>,
        last_tool: Option<String>,
    },
    /// Session recovery initiated
    RecoveryInitiated {
        stage_id: String,
        session_id: String,
        recovery_type: RecoveryType,
    },
    /// Stage has exceeded maximum failure count
    StageEscalated {
        stage_id: String,
        failure_count: u32,
        reason: String,
    },
    /// PreCompact hook fired - context refresh needed
    ContextRefreshNeeded {
        stage_id: String,
        session_id: String,
        context_percent: f32,
    },
}

/// Type of recovery being initiated
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryType {
    /// Recovering from a crashed session (PID dead)
    Crash,
    /// Recovering from a hung session (PID alive, no heartbeat)
    Hung,
    /// Recovering from context exhaustion (graceful refresh)
    ContextRefresh,
    /// Manual recovery triggered by user
    Manual,
}
