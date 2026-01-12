//! Failure tracking and escalation for stages.
//!
//! Tracks consecutive failures per stage and escalates to Blocked status
//! after a configurable threshold (default: 3 failures).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::failure::{FailureInfo, FailureType};

/// Default maximum consecutive failures before escalation
pub const DEFAULT_MAX_FAILURES: u32 = 3;

/// Failure state for a single stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageFailureState {
    /// Stage ID
    pub stage_id: String,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// History of recent failures (last 10)
    pub failure_history: Vec<FailureRecord>,
    /// When the stage was last escalated
    pub last_escalation: Option<DateTime<Utc>>,
    /// Whether the stage is currently escalated
    pub is_escalated: bool,
}

/// Record of a single failure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    /// When the failure occurred
    pub timestamp: DateTime<Utc>,
    /// Type of failure
    pub failure_type: FailureType,
    /// Session ID that failed
    pub session_id: String,
    /// Brief description of the failure
    pub description: String,
}

impl StageFailureState {
    /// Create new failure state for a stage
    pub fn new(stage_id: String) -> Self {
        Self {
            stage_id,
            consecutive_failures: 0,
            failure_history: Vec::new(),
            last_escalation: None,
            is_escalated: false,
        }
    }

    /// Record a failure
    pub fn record_failure(
        &mut self,
        session_id: String,
        failure_type: FailureType,
        description: String,
    ) {
        self.consecutive_failures += 1;

        let record = FailureRecord {
            timestamp: Utc::now(),
            failure_type,
            session_id,
            description,
        };

        self.failure_history.push(record);

        // Keep only last 10 failures
        if self.failure_history.len() > 10 {
            self.failure_history.remove(0);
        }
    }

    /// Reset failure count (called on successful completion or manual reset)
    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.is_escalated = false;
    }

    /// Check if stage should be escalated
    pub fn should_escalate(&self, max_failures: u32) -> bool {
        self.consecutive_failures >= max_failures && !self.is_escalated
    }

    /// Mark as escalated
    pub fn mark_escalated(&mut self) {
        self.is_escalated = true;
        self.last_escalation = Some(Utc::now());
    }

    /// Get the most recent failure
    pub fn last_failure(&self) -> Option<&FailureRecord> {
        self.failure_history.last()
    }
}

/// Tracker for all stage failures
#[derive(Debug, Default)]
pub struct FailureTracker {
    /// Failure states by stage ID
    states: HashMap<String, StageFailureState>,
    /// Maximum consecutive failures before escalation
    max_failures: u32,
}

impl FailureTracker {
    /// Create a new failure tracker with default threshold
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            max_failures: DEFAULT_MAX_FAILURES,
        }
    }

    /// Create with custom threshold
    pub fn with_max_failures(max_failures: u32) -> Self {
        Self {
            states: HashMap::new(),
            max_failures,
        }
    }

    /// Record a failure for a stage
    ///
    /// Returns true if this failure triggers an escalation
    pub fn record_failure(
        &mut self,
        stage_id: &str,
        session_id: String,
        failure_type: FailureType,
        description: String,
    ) -> bool {
        let state = self
            .states
            .entry(stage_id.to_string())
            .or_insert_with(|| StageFailureState::new(stage_id.to_string()));

        state.record_failure(session_id, failure_type, description);

        if state.should_escalate(self.max_failures) {
            state.mark_escalated();
            true
        } else {
            false
        }
    }

    /// Reset failure count for a stage (on success or manual reset)
    pub fn reset_stage(&mut self, stage_id: &str) {
        if let Some(state) = self.states.get_mut(stage_id) {
            state.reset();
        }
    }

    /// Get failure state for a stage
    pub fn get_state(&self, stage_id: &str) -> Option<&StageFailureState> {
        self.states.get(stage_id)
    }

    /// Get mutable failure state for a stage
    pub fn get_state_mut(&mut self, stage_id: &str) -> Option<&mut StageFailureState> {
        self.states.get_mut(stage_id)
    }

    /// Check if a stage is currently escalated
    pub fn is_escalated(&self, stage_id: &str) -> bool {
        self.states
            .get(stage_id)
            .map(|s| s.is_escalated)
            .unwrap_or(false)
    }

    /// Get consecutive failure count for a stage
    pub fn failure_count(&self, stage_id: &str) -> u32 {
        self.states
            .get(stage_id)
            .map(|s| s.consecutive_failures)
            .unwrap_or(0)
    }

    /// Load state from .work/state/<stage-id>.yaml files
    pub fn load_from_work_dir(&mut self, work_dir: &Path) -> Result<()> {
        let state_dir = work_dir.join("state");
        if !state_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&state_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
                continue;
            }

            match load_failure_state(&path) {
                Ok(state) => {
                    self.states.insert(state.stage_id.clone(), state);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load failure state from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Save state to .work/state/<stage-id>.yaml
    pub fn save_to_work_dir(&self, work_dir: &Path) -> Result<()> {
        let state_dir = work_dir.join("state");
        if !state_dir.exists() {
            fs::create_dir_all(&state_dir).with_context(|| {
                format!("Failed to create state directory: {}", state_dir.display())
            })?;
        }

        for state in self.states.values() {
            save_failure_state(state, &state_dir)?;
        }

        Ok(())
    }

    /// Save state for a specific stage
    pub fn save_stage_state(&self, stage_id: &str, work_dir: &Path) -> Result<()> {
        let state_dir = work_dir.join("state");
        if !state_dir.exists() {
            fs::create_dir_all(&state_dir).with_context(|| {
                format!("Failed to create state directory: {}", state_dir.display())
            })?;
        }

        if let Some(state) = self.states.get(stage_id) {
            save_failure_state(state, &state_dir)?;
        }

        Ok(())
    }
}

/// Load failure state from a YAML file
fn load_failure_state(path: &Path) -> Result<StageFailureState> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read failure state: {}", path.display()))?;
    serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse failure state: {}", path.display()))
}

/// Save failure state to a YAML file
fn save_failure_state(state: &StageFailureState, state_dir: &Path) -> Result<()> {
    let path = state_dir.join(format!("{}.yaml", state.stage_id));
    let content = serde_yaml::to_string(state).context("Failed to serialize failure state")?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write failure state: {}", path.display()))
}

/// Get failure state path for a stage
pub fn failure_state_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join("state").join(format!("{stage_id}.yaml"))
}

/// Build a FailureInfo from a failure record
pub fn build_failure_info(record: &FailureRecord) -> FailureInfo {
    FailureInfo {
        failure_type: record.failure_type.clone(),
        detected_at: record.timestamp,
        evidence: vec![record.description.clone()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_failure_recording() {
        let mut tracker = FailureTracker::with_max_failures(3);

        // Record first two failures - should not escalate
        let escalated = tracker.record_failure(
            "stage-1",
            "session-1".to_string(),
            FailureType::SessionCrash,
            "First crash".to_string(),
        );
        assert!(!escalated);
        assert_eq!(tracker.failure_count("stage-1"), 1);

        let escalated = tracker.record_failure(
            "stage-1",
            "session-2".to_string(),
            FailureType::SessionCrash,
            "Second crash".to_string(),
        );
        assert!(!escalated);
        assert_eq!(tracker.failure_count("stage-1"), 2);

        // Third failure should trigger escalation
        let escalated = tracker.record_failure(
            "stage-1",
            "session-3".to_string(),
            FailureType::SessionCrash,
            "Third crash".to_string(),
        );
        assert!(escalated);
        assert_eq!(tracker.failure_count("stage-1"), 3);
        assert!(tracker.is_escalated("stage-1"));
    }

    #[test]
    fn test_reset() {
        let mut tracker = FailureTracker::with_max_failures(3);

        // Record some failures
        for i in 1..=3 {
            tracker.record_failure(
                "stage-1",
                format!("session-{i}"),
                FailureType::SessionCrash,
                format!("Crash {i}"),
            );
        }

        assert!(tracker.is_escalated("stage-1"));
        assert_eq!(tracker.failure_count("stage-1"), 3);

        // Reset
        tracker.reset_stage("stage-1");

        assert!(!tracker.is_escalated("stage-1"));
        assert_eq!(tracker.failure_count("stage-1"), 0);
    }

    #[test]
    fn test_save_and_load() -> Result<()> {
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        let mut tracker = FailureTracker::with_max_failures(3);
        tracker.record_failure(
            "stage-1",
            "session-1".to_string(),
            FailureType::SessionCrash,
            "Test crash".to_string(),
        );

        tracker.save_to_work_dir(work_dir)?;

        // Create new tracker and load
        let mut loaded_tracker = FailureTracker::with_max_failures(3);
        loaded_tracker.load_from_work_dir(work_dir)?;

        assert_eq!(loaded_tracker.failure_count("stage-1"), 1);

        Ok(())
    }
}
