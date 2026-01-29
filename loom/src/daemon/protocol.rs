use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

use crate::models::stage::StageStatus;
use crate::models::worktree::WorktreeStatus;

/// Information about a single stage's completion status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageCompletionInfo {
    /// Stage identifier
    pub id: String,
    /// Human-readable stage name
    pub name: String,
    /// Final status of the stage
    pub status: StageStatus,
    /// Duration in seconds from start to completion (None if never started)
    pub duration_secs: Option<i64>,
    /// Whether the stage was merged
    pub merged: bool,
    /// Dependencies of this stage
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Summary of orchestration completion.
///
/// Sent to all status subscribers when the orchestrator finishes
/// executing all stages (successfully or with failures).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionSummary {
    /// Total orchestration duration in seconds
    pub total_duration_secs: i64,
    /// Completion info for each stage
    pub stages: Vec<StageCompletionInfo>,
    /// Number of successfully completed stages
    pub success_count: usize,
    /// Number of failed/blocked stages
    pub failure_count: usize,
    /// Path to the plan that was executed
    pub plan_path: String,
}

/// Configuration parameters for daemon mode.
///
/// These parameters control how the daemon executes stages,
/// matching the CLI flags available with `loom run`.
///
/// Note: Configuration is set when the daemon starts and cannot be
/// changed at runtime. To change configuration, stop the daemon
/// with `loom stop` and restart it with `loom run` using the
/// desired flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Manual mode - don't auto-start stages (maps to --manual)
    pub manual_mode: bool,
    /// Maximum concurrent stages (maps to --max-parallel)
    pub max_parallel: Option<usize>,
    /// Watch mode - monitor for changes (maps to --watch)
    pub watch_mode: bool,
    /// Auto-merge completed stages (default: true, disable with --no-merge)
    pub auto_merge: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            manual_mode: false,
            max_parallel: None,
            watch_mode: true,
            auto_merge: true,
        }
    }
}

/// Client request to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Subscribe to live status updates
    SubscribeStatus,
    /// Subscribe to raw log stream
    SubscribeLogs,
    /// Request daemon shutdown
    Stop,
    /// Disconnect cleanly
    Unsubscribe,
    /// Ping to check if daemon is alive
    Ping,
}

/// Daemon response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Error {
        message: String,
    },
    StatusUpdate {
        stages_executing: Vec<StageInfo>,
        stages_pending: Vec<StageInfo>,
        stages_completed: Vec<StageInfo>,
        stages_blocked: Vec<StageInfo>,
    },
    /// Orchestration has completed (all stages terminal)
    OrchestrationComplete {
        summary: CompletionSummary,
    },
    LogLine {
        line: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageInfo {
    pub id: String,
    pub name: String,
    pub session_pid: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub worktree_status: Option<WorktreeStatus>,
    /// Current status of the stage in the execution lifecycle
    pub status: StageStatus,
    /// Whether this stage's changes have been merged to the merge point
    #[serde(default)]
    pub merged: bool,
    /// IDs of stages this stage depends on
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Write a length-prefixed JSON message to a stream.
///
/// Format: 4-byte big-endian length prefix + JSON data
///
/// # Arguments
/// * `stream` - The stream to write to
/// * `message` - The message to serialize and write
///
/// # Returns
/// `Ok(())` on success, error if serialization or write fails
pub fn write_message<T: Serialize, W: Write>(stream: &mut W, message: &T) -> Result<()> {
    let json = serde_json::to_vec(message).context("Failed to serialize message")?;
    let len = json.len() as u32;
    let len_bytes = len.to_be_bytes();

    stream
        .write_all(&len_bytes)
        .context("Failed to write message length")?;
    stream
        .write_all(&json)
        .context("Failed to write message body")?;
    stream.flush().context("Failed to flush stream")?;

    Ok(())
}

/// Read a length-prefixed JSON message from a stream.
///
/// Format: 4-byte big-endian length prefix + JSON data
///
/// # Arguments
/// * `stream` - The stream to read from
///
/// # Returns
/// `Ok(T)` with the deserialized message on success, error if read or deserialization fails
pub fn read_message<T: for<'de> Deserialize<'de>, R: Read>(stream: &mut R) -> Result<T> {
    let mut len_bytes = [0u8; 4];
    stream
        .read_exact(&mut len_bytes)
        .context("Failed to read message length")?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Sanity check: prevent DOS via huge length claim (max 10 MB)
    if len > 10 * 1024 * 1024 {
        bail!("Message too large: {len} bytes");
    }

    let mut json_bytes = vec![0u8; len];
    stream
        .read_exact(&mut json_bytes)
        .context("Failed to read message body")?;

    serde_json::from_slice(&json_bytes).context("Failed to deserialize message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use crate::models::worktree::WorktreeStatus;
    use std::io::Cursor;

    #[test]
    fn test_write_and_read_request() {
        let mut buffer = Vec::new();
        let request = Request::Ping;

        write_message(&mut buffer, &request).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Request = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Request::Ping => {}
            _ => panic!("Expected Ping request"),
        }
    }

    #[test]
    fn test_write_and_read_response() {
        let mut buffer = Vec::new();
        let response = Response::Pong;

        write_message(&mut buffer, &response).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Response = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Response::Pong => {}
            _ => panic!("Expected Pong response"),
        }
    }

    #[test]
    fn test_write_and_read_status_update() {
        let mut buffer = Vec::new();
        let now = Utc::now();
        let response = Response::StatusUpdate {
            stages_executing: vec![StageInfo {
                id: "stage-1".to_string(),
                name: "Test Stage".to_string(),
                session_pid: Some(12345),
                started_at: now,
                completed_at: None,
                worktree_status: Some(WorktreeStatus::Active),
                status: StageStatus::Executing,
                merged: false,
                dependencies: vec!["stage-0".to_string()],
            }],
            stages_pending: vec![StageInfo {
                id: "stage-2".to_string(),
                name: "Pending Stage".to_string(),
                session_pid: None,
                started_at: now,
                completed_at: None,
                worktree_status: None,
                status: StageStatus::WaitingForDeps,
                merged: false,
                dependencies: vec!["stage-1".to_string()],
            }],
            stages_completed: vec![StageInfo {
                id: "stage-0".to_string(),
                name: "Completed Stage".to_string(),
                session_pid: None,
                started_at: now,
                completed_at: Some(now),
                worktree_status: None,
                status: StageStatus::Completed,
                merged: true,
                dependencies: vec![],
            }],
            stages_blocked: vec![],
        };

        write_message(&mut buffer, &response).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Response = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Response::StatusUpdate {
                stages_executing, ..
            } => {
                assert_eq!(stages_executing.len(), 1);
                assert_eq!(stages_executing[0].id, "stage-1");
            }
            _ => panic!("Expected StatusUpdate response"),
        }
    }

    #[test]
    fn test_read_message_too_large() {
        let mut buffer = Vec::new();
        let len: u32 = 20 * 1024 * 1024; // 20 MB (exceeds 10 MB limit)
        buffer.extend_from_slice(&len.to_be_bytes());

        let mut cursor = Cursor::new(buffer);
        let result: Result<Request> = read_message(&mut cursor);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();

        assert!(!config.manual_mode);
        assert!(config.max_parallel.is_none());
        assert!(config.watch_mode);
        assert!(config.auto_merge);
    }

    #[test]
    fn test_write_and_read_orchestration_complete() {
        use super::{CompletionSummary, StageCompletionInfo};

        let mut buffer = Vec::new();
        let response = Response::OrchestrationComplete {
            summary: CompletionSummary {
                total_duration_secs: 120,
                stages: vec![
                    StageCompletionInfo {
                        id: "stage-1".to_string(),
                        name: "First Stage".to_string(),
                        status: StageStatus::Completed,
                        duration_secs: Some(60),
                        merged: true,
                        dependencies: vec![],
                    },
                    StageCompletionInfo {
                        id: "stage-2".to_string(),
                        name: "Second Stage".to_string(),
                        status: StageStatus::Blocked,
                        duration_secs: Some(45),
                        merged: false,
                        dependencies: vec!["stage-1".to_string()],
                    },
                ],
                success_count: 1,
                failure_count: 1,
                plan_path: "doc/plans/PLAN-test.md".to_string(),
            },
        };

        write_message(&mut buffer, &response).expect("Failed to write message");

        let mut cursor = Cursor::new(buffer);
        let decoded: Response = read_message(&mut cursor).expect("Failed to read message");

        match decoded {
            Response::OrchestrationComplete { summary } => {
                assert_eq!(summary.total_duration_secs, 120);
                assert_eq!(summary.stages.len(), 2);
                assert_eq!(summary.stages[0].id, "stage-1");
                assert_eq!(summary.stages[0].status, StageStatus::Completed);
                assert_eq!(summary.stages[1].id, "stage-2");
                assert_eq!(summary.stages[1].status, StageStatus::Blocked);
                assert_eq!(summary.success_count, 1);
                assert_eq!(summary.failure_count, 1);
                assert_eq!(summary.plan_path, "doc/plans/PLAN-test.md");
            }
            _ => panic!("Expected OrchestrationComplete response"),
        }
    }
}
