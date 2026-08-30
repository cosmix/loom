//! Heartbeat protocol for session health monitoring.
//!
//! Sessions write heartbeat files to `.work/heartbeat/<stage-id>.json` to indicate
//! they are still actively working. The heartbeat includes:
//! - Timestamp of last activity
//! - Resident context, in absolute tokens
//! - Path to the agent's transcript
//! - Last tool used
//!
//! The orchestrator polls these files to detect:
//! - Crashed sessions (PID dead)
//! - Hung sessions (PID alive but no heartbeat update for threshold duration)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default timeout for considering a session hung (5 minutes)
pub const DEFAULT_HUNG_TIMEOUT_SECS: u64 = 300;

/// Default polling interval for heartbeat checks (10 seconds)
pub const DEFAULT_HEARTBEAT_POLL_SECS: u64 = 10;

/// Heartbeat data written by Claude Code hooks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// Stage ID this heartbeat is for
    pub stage_id: String,
    /// Session ID
    pub session_id: String,
    /// Timestamp of this heartbeat
    pub timestamp: DateTime<Utc>,
    /// Resident context in absolute tokens, as measured from the transcript.
    /// `None` means the hook could not measure it on this tick — it is not a
    /// reading of zero, and consumers must preserve the previous value.
    #[serde(default)]
    pub context_tokens: Option<u32>,
    /// Path to the agent's transcript file, as reported by the hook.
    #[serde(default)]
    pub transcript_path: Option<String>,
    /// Last tool that was used
    #[serde(default)]
    pub last_tool: Option<String>,
    /// Optional message about current activity
    #[serde(default)]
    pub activity: Option<String>,
}

impl Heartbeat {
    /// Create a new heartbeat
    pub fn new(stage_id: String, session_id: String) -> Self {
        Self {
            stage_id,
            session_id,
            timestamp: Utc::now(),
            context_tokens: None,
            transcript_path: None,
            last_tool: None,
            activity: None,
        }
    }

    /// Create heartbeat with a resident-token reading
    pub fn with_context_tokens(mut self, tokens: u32) -> Self {
        self.context_tokens = Some(tokens);
        self
    }

    /// Create heartbeat with the agent's transcript path
    pub fn with_transcript_path(mut self, path: impl Into<String>) -> Self {
        self.transcript_path = Some(path.into());
        self
    }

    /// Create heartbeat with last tool
    pub fn with_last_tool(mut self, tool: String) -> Self {
        self.last_tool = Some(tool);
        self
    }

    /// Create heartbeat with activity message
    pub fn with_activity(mut self, activity: String) -> Self {
        self.activity = Some(activity);
        self
    }

    /// Check if heartbeat is stale (older than timeout)
    pub fn is_stale(&self, timeout: Duration) -> bool {
        let age = Utc::now().signed_duration_since(self.timestamp);
        if let Ok(timeout_chrono) = chrono::Duration::from_std(timeout) {
            age > timeout_chrono
        } else {
            false
        }
    }

    /// Get the age of this heartbeat
    pub fn age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.timestamp)
    }
}

/// Result of checking a session's health via heartbeat
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatStatus {
    /// Session is healthy - recent heartbeat received
    Healthy,
    /// Session appears hung - PID alive but no recent heartbeat
    Hung {
        /// How long since last heartbeat
        stale_duration_secs: u64,
    },
    /// No heartbeat file exists (session may not have started heartbeat yet)
    NoHeartbeat,
}

/// Watches heartbeat files and tracks session health.
///
/// The watcher holds no timeout of its own: the staleness threshold is
/// per-stage (`Stage::subagent_timeout_secs`) and one watcher serves every
/// stage, so callers resolve the budget and pass it to
/// [`HeartbeatWatcher::check_session_hung`].
#[derive(Debug)]
pub struct HeartbeatWatcher {
    /// Cached heartbeats by stage ID
    heartbeats: HashMap<String, Heartbeat>,
}

/// Hook timestamps have whole-second precision, so every heartbeat field is
/// part of the update discriminator. Context can change twice in one second
/// across native compaction even when the timestamp does not.
fn heartbeat_changed(previous: Option<&Heartbeat>, current: &Heartbeat) -> bool {
    previous != Some(current)
}

impl HeartbeatWatcher {
    /// Create a new heartbeat watcher
    pub fn new() -> Self {
        Self {
            heartbeats: HashMap::new(),
        }
    }

    /// Poll heartbeat files and update cache
    pub fn poll(&mut self, work_dir: &Path) -> Result<Vec<HeartbeatUpdate>> {
        let heartbeat_dir = work_dir.join("heartbeat");
        if !heartbeat_dir.exists() {
            return Ok(Vec::new());
        }

        let mut updates = Vec::new();

        for entry in std::fs::read_dir(&heartbeat_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let stage_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            match read_heartbeat(&path) {
                Ok(heartbeat) => {
                    let previous = self.heartbeats.get(&stage_id);
                    let is_new = previous.is_none();
                    let is_updated = heartbeat_changed(previous, &heartbeat);

                    if is_new || is_updated {
                        updates.push(HeartbeatUpdate {
                            stage_id: stage_id.clone(),
                            heartbeat: heartbeat.clone(),
                            is_new,
                        });
                    }

                    self.heartbeats.insert(stage_id, heartbeat);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read heartbeat {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        Ok(updates)
    }

    /// Get the heartbeat for a stage
    pub fn get_heartbeat(&self, stage_id: &str) -> Option<&Heartbeat> {
        self.heartbeats.get(stage_id)
    }

    /// Check if a session is hung based on heartbeat staleness.
    ///
    /// `timeout` is the stage's response budget, resolved by the caller via
    /// [`Stage::effective_subagent_timeout_secs`]. It is a parameter rather than
    /// watcher state because the threshold is per-stage while one watcher serves
    /// every stage.
    ///
    /// `session_id` is the ID of the session currently occupying the stage.
    /// Heartbeat files are keyed by stage ID, so a heartbeat written by a
    /// previous session for the same stage can linger after that session
    /// exits. If the cached heartbeat's `session_id` does not match the
    /// session we are checking, it belongs to a stale/previous session and
    /// must NOT flag the fresh session as hung — treat it as `NoHeartbeat`
    /// (the fresh session has simply not written its own heartbeat yet).
    ///
    /// [`Stage::effective_subagent_timeout_secs`]: crate::models::stage::Stage::effective_subagent_timeout_secs
    pub fn check_session_hung(
        &self,
        stage_id: &str,
        session_id: &str,
        timeout: Duration,
    ) -> HeartbeatStatus {
        match self.heartbeats.get(stage_id) {
            None => HeartbeatStatus::NoHeartbeat,
            Some(heartbeat) if heartbeat.session_id != session_id => {
                // Stale heartbeat from a previous session for this stage.
                HeartbeatStatus::NoHeartbeat
            }
            Some(heartbeat) => {
                if heartbeat.is_stale(timeout) {
                    let age = heartbeat.age();
                    HeartbeatStatus::Hung {
                        stale_duration_secs: age.num_seconds().max(0) as u64,
                    }
                } else {
                    HeartbeatStatus::Healthy
                }
            }
        }
    }

    /// Remove heartbeat for a stage (when session ends)
    pub fn remove(&mut self, stage_id: &str) {
        self.heartbeats.remove(stage_id);
    }

    /// Get all cached heartbeats
    pub fn all_heartbeats(&self) -> &HashMap<String, Heartbeat> {
        &self.heartbeats
    }
}

impl Default for HeartbeatWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Update from polling heartbeat files
#[derive(Debug, Clone)]
pub struct HeartbeatUpdate {
    /// Stage ID
    pub stage_id: String,
    /// The heartbeat data
    pub heartbeat: Heartbeat,
    /// Whether this is a new heartbeat (first seen)
    pub is_new: bool,
}

/// Read a heartbeat file
pub fn read_heartbeat(path: &Path) -> Result<Heartbeat> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read heartbeat file: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse heartbeat file: {}", path.display()))
}

/// Write a heartbeat file
pub fn write_heartbeat(work_dir: &Path, heartbeat: &Heartbeat) -> Result<PathBuf> {
    let heartbeat_dir = work_dir.join("heartbeat");
    if !heartbeat_dir.exists() {
        std::fs::create_dir_all(&heartbeat_dir).with_context(|| {
            format!(
                "Failed to create heartbeat directory: {}",
                heartbeat_dir.display()
            )
        })?;
    }

    let path = heartbeat_dir.join(format!("{}.json", heartbeat.stage_id));
    let content =
        serde_json::to_string_pretty(heartbeat).context("Failed to serialize heartbeat")?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write heartbeat file: {}", path.display()))?;

    Ok(path)
}

/// Remove a heartbeat file
pub fn remove_heartbeat(work_dir: &Path, stage_id: &str) -> Result<()> {
    let path = work_dir.join("heartbeat").join(format!("{stage_id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove heartbeat file: {}", path.display()))?;
    }
    Ok(())
}

/// Get heartbeat path for a stage
pub fn heartbeat_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join("heartbeat").join(format!("{stage_id}.json"))
}

/// Read the resident-token count from a stage's latest heartbeat file.
///
/// `None` covers every way the reading can be unavailable — no heartbeat file,
/// an unreadable one, or a hook that could not measure the transcript — because
/// callers treat all three the same way: they have no reading, not a reading of
/// zero.
pub fn stage_context_tokens(work_dir: &Path, stage_id: &str) -> Option<u32> {
    read_heartbeat(&heartbeat_path(work_dir, stage_id))
        .ok()
        .and_then(|heartbeat| heartbeat.context_tokens)
}

#[cfg(test)]
mod tests;
