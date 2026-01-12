//! Heartbeat protocol for session health monitoring.
//!
//! Sessions write heartbeat files to `.work/heartbeat/<stage-id>.json` to indicate
//! they are still actively working. The heartbeat includes:
//! - Timestamp of last activity
//! - Context usage percentage
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    /// Stage ID this heartbeat is for
    pub stage_id: String,
    /// Session ID
    pub session_id: String,
    /// Timestamp of this heartbeat
    pub timestamp: DateTime<Utc>,
    /// Context usage percentage (0-100)
    #[serde(default)]
    pub context_percent: Option<f32>,
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
            context_percent: None,
            last_tool: None,
            activity: None,
        }
    }

    /// Create heartbeat with context percentage
    pub fn with_context_percent(mut self, percent: f32) -> Self {
        self.context_percent = Some(percent);
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
    /// Session crashed - PID is dead
    Crashed,
    /// No heartbeat file exists (session may not have started heartbeat yet)
    NoHeartbeat,
}

/// Watches heartbeat files and tracks session health
#[derive(Debug)]
pub struct HeartbeatWatcher {
    /// Cached heartbeats by stage ID
    heartbeats: HashMap<String, Heartbeat>,
    /// Timeout for considering a session hung
    hung_timeout: Duration,
}

impl HeartbeatWatcher {
    /// Create a new heartbeat watcher with default timeout
    pub fn new() -> Self {
        Self {
            heartbeats: HashMap::new(),
            hung_timeout: Duration::from_secs(DEFAULT_HUNG_TIMEOUT_SECS),
        }
    }

    /// Create with custom hung timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            heartbeats: HashMap::new(),
            hung_timeout: timeout,
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
                    let is_updated = previous
                        .map(|p| p.timestamp != heartbeat.timestamp)
                        .unwrap_or(true);

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

    /// Check if a session is hung based on heartbeat staleness
    pub fn check_session_hung(&self, stage_id: &str) -> HeartbeatStatus {
        match self.heartbeats.get(stage_id) {
            None => HeartbeatStatus::NoHeartbeat,
            Some(heartbeat) => {
                if heartbeat.is_stale(self.hung_timeout) {
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

    /// Set the hung timeout
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.hung_timeout = timeout;
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
    let path = work_dir
        .join("heartbeat")
        .join(format!("{stage_id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove heartbeat file: {}", path.display()))?;
    }
    Ok(())
}

/// Get heartbeat path for a stage
pub fn heartbeat_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir
        .join("heartbeat")
        .join(format!("{stage_id}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_heartbeat_creation() {
        let hb = Heartbeat::new("stage-1".to_string(), "session-abc".to_string())
            .with_context_percent(45.5)
            .with_last_tool("Bash".to_string())
            .with_activity("Running tests".to_string());

        assert_eq!(hb.stage_id, "stage-1");
        assert_eq!(hb.session_id, "session-abc");
        assert_eq!(hb.context_percent, Some(45.5));
        assert_eq!(hb.last_tool, Some("Bash".to_string()));
        assert_eq!(hb.activity, Some("Running tests".to_string()));
    }

    #[test]
    fn test_heartbeat_staleness() {
        let hb = Heartbeat::new("stage-1".to_string(), "session-abc".to_string());

        // Fresh heartbeat should not be stale
        assert!(!hb.is_stale(Duration::from_secs(300)));

        // Any heartbeat is stale with 0 timeout
        assert!(hb.is_stale(Duration::from_secs(0)));
    }

    #[test]
    fn test_write_and_read_heartbeat() -> Result<()> {
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        let hb = Heartbeat::new("test-stage".to_string(), "test-session".to_string())
            .with_context_percent(50.0);

        let path = write_heartbeat(work_dir, &hb)?;
        assert!(path.exists());

        let read_hb = read_heartbeat(&path)?;
        assert_eq!(read_hb.stage_id, "test-stage");
        assert_eq!(read_hb.session_id, "test-session");
        assert_eq!(read_hb.context_percent, Some(50.0));

        Ok(())
    }

    #[test]
    fn test_heartbeat_watcher_poll() -> Result<()> {
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        // Write a heartbeat
        let hb = Heartbeat::new("stage-1".to_string(), "session-1".to_string());
        write_heartbeat(work_dir, &hb)?;

        // Poll should find it
        let mut watcher = HeartbeatWatcher::new();
        let updates = watcher.poll(work_dir)?;

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].stage_id, "stage-1");
        assert!(updates[0].is_new);

        // Second poll should not return update (no change)
        let updates = watcher.poll(work_dir)?;
        assert!(updates.is_empty());

        Ok(())
    }

    #[test]
    fn test_heartbeat_watcher_check_hung() {
        let mut watcher = HeartbeatWatcher::with_timeout(Duration::from_secs(60));

        // No heartbeat
        assert_eq!(
            watcher.check_session_hung("unknown"),
            HeartbeatStatus::NoHeartbeat
        );

        // Add a fresh heartbeat
        let hb = Heartbeat::new("stage-1".to_string(), "session-1".to_string());
        watcher.heartbeats.insert("stage-1".to_string(), hb);

        assert_eq!(
            watcher.check_session_hung("stage-1"),
            HeartbeatStatus::Healthy
        );

        // Add an old heartbeat (simulate by setting timeout to 0)
        watcher.set_timeout(Duration::from_secs(0));
        match watcher.check_session_hung("stage-1") {
            HeartbeatStatus::Hung { .. } => (),
            other => panic!("Expected Hung, got {other:?}"),
        }
    }
}
