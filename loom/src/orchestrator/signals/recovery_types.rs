//! Types for recovery signal generation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of recovery being initiated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryReason {
    /// Session crashed (PID dead)
    Crash,
    /// Session hung (PID alive, no heartbeat)
    Hung,
    /// Context exhaustion (PreCompact fired)
    ContextExhaustion,
    /// Manual recovery triggered by user
    Manual,
}

impl std::fmt::Display for RecoveryReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryReason::Crash => write!(f, "Session crashed"),
            RecoveryReason::Hung => write!(f, "Session hung"),
            RecoveryReason::ContextExhaustion => write!(f, "Context exhaustion"),
            RecoveryReason::Manual => write!(f, "Manual recovery"),
        }
    }
}

/// Content for a recovery signal
#[derive(Debug, Clone)]
pub struct RecoverySignalContent {
    /// New session ID for the recovery session
    pub session_id: String,
    /// Stage being recovered
    pub stage_id: String,
    /// Previous session ID that crashed/hung
    pub previous_session_id: String,
    /// Reason for recovery
    pub reason: RecoveryReason,
    /// Time when the issue was detected
    pub detected_at: DateTime<Utc>,
    /// Last heartbeat information (if available)
    pub last_heartbeat: Option<LastHeartbeatInfo>,
    /// Crash report path (if available)
    pub crash_report_path: Option<PathBuf>,
    /// Suggested recovery actions
    pub recovery_actions: Vec<String>,
    /// How many times this stage has been recovered
    pub recovery_attempt: u32,
}

/// Information from the last heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastHeartbeatInfo {
    /// When the heartbeat was recorded
    pub timestamp: DateTime<Utc>,
    /// Context percentage at the time
    pub context_percent: Option<f32>,
    /// Last tool being used
    pub last_tool: Option<String>,
    /// Activity description
    pub activity: Option<String>,
}

impl RecoverySignalContent {
    /// Create a new recovery signal for a crashed session
    pub fn for_crash(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        crash_report_path: Option<PathBuf>,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Crash,
            detected_at: Utc::now(),
            last_heartbeat: None,
            crash_report_path,
            recovery_actions: vec![
                "Review the crash report for error details".to_string(),
                "Continue work from the last known state".to_string(),
                "If the issue persists, check for environmental problems".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for a hung session
    pub fn for_hung(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        last_heartbeat: Option<LastHeartbeatInfo>,
        recovery_attempt: u32,
    ) -> Self {
        let mut recovery_actions = vec![
            "Review the last known activity before hang".to_string(),
            "Check if the operation was waiting for external resources".to_string(),
        ];

        if let Some(ref hb) = last_heartbeat {
            if let Some(ref tool) = hb.last_tool {
                recovery_actions.insert(0, format!("Previous session was using: {tool}"));
            }
        }

        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Hung,
            detected_at: Utc::now(),
            last_heartbeat,
            crash_report_path: None,
            recovery_actions,
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for context exhaustion
    pub fn for_context_exhaustion(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        context_percent: f32,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::ContextExhaustion,
            detected_at: Utc::now(),
            last_heartbeat: Some(LastHeartbeatInfo {
                timestamp: Utc::now(),
                context_percent: Some(context_percent),
                last_tool: None,
                activity: Some("Context limit reached".to_string()),
            }),
            crash_report_path: None,
            recovery_actions: vec![
                "Read the handoff file carefully for context".to_string(),
                "Continue from the documented progress".to_string(),
                "Prioritize completing remaining tasks efficiently".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for manual recovery
    pub fn for_manual(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Manual,
            detected_at: Utc::now(),
            last_heartbeat: None,
            crash_report_path: None,
            recovery_actions: vec![
                "Review any available handoff or crash reports".to_string(),
                "Check the current state of the stage's work".to_string(),
                "Continue from where the previous session left off".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Set custom recovery actions
    pub fn with_recovery_actions(mut self, actions: Vec<String>) -> Self {
        self.recovery_actions = actions;
        self
    }

    /// Add a recovery action
    pub fn add_recovery_action(&mut self, action: String) {
        self.recovery_actions.push(action);
    }
}
