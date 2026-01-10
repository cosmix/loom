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
}
