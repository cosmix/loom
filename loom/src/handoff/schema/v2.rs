//! HandoffV2 schema definition and builder methods.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::types::{CommitRef, CompletedTask, FileRef, KeyDecision};

/// Version of the handoff schema
pub const HANDOFF_SCHEMA_VERSION: u32 = 2;

/// Structured handoff schema V2.
///
/// This format replaces prose handoffs with validated YAML fields,
/// enabling reliable parsing and context restoration for continuation sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffV2 {
    /// Schema version (always 2 for this struct)
    pub version: u32,
    /// ID of the session that created this handoff
    pub session_id: String,
    /// ID of the stage being worked on
    pub stage_id: String,
    /// Context usage percentage (0.0 - 100.0) at handoff time
    pub context_percent: f32,
    /// Tasks completed during this session
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_tasks: Vec<CompletedTask>,
    /// Key architectural or implementation decisions made
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_decisions: Vec<KeyDecision>,
    /// Facts discovered about the codebase during work
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discovered_facts: Vec<String>,
    /// Open questions that need resolution
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    /// Prioritized next actions for the continuation session
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    /// Git branch at handoff time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Commits made during this session
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<CommitRef>,
    /// Files with uncommitted changes at handoff time
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncommitted_files: Vec<String>,
    /// Files read for context during the session
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_read: Vec<FileRef>,
    /// Files modified during the session
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_modified: Vec<String>,
}

impl HandoffV2 {
    /// Create a new HandoffV2 with required fields
    pub fn new(session_id: impl Into<String>, stage_id: impl Into<String>) -> Self {
        Self {
            version: HANDOFF_SCHEMA_VERSION,
            session_id: session_id.into(),
            stage_id: stage_id.into(),
            context_percent: 0.0,
            completed_tasks: Vec::new(),
            key_decisions: Vec::new(),
            discovered_facts: Vec::new(),
            open_questions: Vec::new(),
            next_actions: Vec::new(),
            branch: None,
            commits: Vec::new(),
            uncommitted_files: Vec::new(),
            files_read: Vec::new(),
            files_modified: Vec::new(),
        }
    }

    /// Set context percentage
    pub fn with_context_percent(mut self, percent: f32) -> Self {
        self.context_percent = percent;
        self
    }

    /// Set completed tasks
    pub fn with_completed_tasks(mut self, tasks: Vec<CompletedTask>) -> Self {
        self.completed_tasks = tasks;
        self
    }

    /// Set key decisions
    pub fn with_key_decisions(mut self, decisions: Vec<KeyDecision>) -> Self {
        self.key_decisions = decisions;
        self
    }

    /// Set next actions
    pub fn with_next_actions(mut self, actions: Vec<String>) -> Self {
        self.next_actions = actions;
        self
    }

    /// Set branch
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set commits
    pub fn with_commits(mut self, commits: Vec<CommitRef>) -> Self {
        self.commits = commits;
        self
    }

    /// Set uncommitted files
    pub fn with_uncommitted_files(mut self, files: Vec<String>) -> Self {
        self.uncommitted_files = files;
        self
    }

    /// Set files read
    #[cfg(test)]
    pub fn with_files_read(mut self, files: Vec<FileRef>) -> Self {
        self.files_read = files;
        self
    }

    /// Set files modified
    pub fn with_files_modified(mut self, files: Vec<String>) -> Self {
        self.files_modified = files;
        self
    }

    /// Serialize to YAML string
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self).context("Failed to serialize handoff to YAML")
    }

    /// Parse from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let handoff: Self = serde_yaml::from_str(yaml).context("Failed to parse handoff YAML")?;
        handoff.validate()?;
        Ok(handoff)
    }

    /// Validate the handoff data
    pub fn validate(&self) -> Result<()> {
        if self.version != HANDOFF_SCHEMA_VERSION {
            bail!(
                "Unsupported handoff version: {}. Expected {}.",
                self.version,
                HANDOFF_SCHEMA_VERSION
            );
        }

        if self.session_id.is_empty() {
            bail!("Handoff session_id cannot be empty");
        }

        if self.stage_id.is_empty() {
            bail!("Handoff stage_id cannot be empty");
        }

        if !(0.0..=100.0).contains(&self.context_percent) {
            bail!(
                "Handoff context_percent must be between 0.0 and 100.0, got {}",
                self.context_percent
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handoff_v2_new() {
        let handoff = HandoffV2::new("session-123", "stage-1");
        assert_eq!(handoff.version, HANDOFF_SCHEMA_VERSION);
        assert_eq!(handoff.session_id, "session-123");
        assert_eq!(handoff.stage_id, "stage-1");
        assert_eq!(handoff.context_percent, 0.0);
    }

    #[test]
    fn test_handoff_v2_builder() {
        let handoff = HandoffV2::new("session-123", "stage-1")
            .with_context_percent(75.5)
            .with_branch("loom/stage-1")
            .with_completed_tasks(vec![CompletedTask::new("Task 1")])
            .with_next_actions(vec!["Continue work".to_string()]);

        assert_eq!(handoff.context_percent, 75.5);
        assert_eq!(handoff.branch, Some("loom/stage-1".to_string()));
        assert_eq!(handoff.completed_tasks.len(), 1);
        assert_eq!(handoff.next_actions.len(), 1);
    }

    #[test]
    fn test_handoff_v2_yaml_roundtrip() {
        let original = HandoffV2::new("session-abc", "my-stage")
            .with_context_percent(65.0)
            .with_branch("loom/my-stage")
            .with_completed_tasks(vec![CompletedTask::new("Did something")])
            .with_key_decisions(vec![KeyDecision::new(
                "Used pattern X",
                "Better performance",
            )])
            .with_files_read(vec![FileRef::with_lines("src/main.rs", 1, 100, "context")]);

        let yaml = original.to_yaml().unwrap();
        let parsed = HandoffV2::from_yaml(&yaml).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_handoff_v2_validation() {
        // Valid handoff
        let handoff = HandoffV2::new("session-1", "stage-1").with_context_percent(50.0);
        assert!(handoff.validate().is_ok());

        // Invalid: empty session_id
        let mut invalid = HandoffV2::new("", "stage-1");
        assert!(invalid.validate().is_err());

        // Invalid: empty stage_id
        invalid = HandoffV2::new("session-1", "");
        assert!(invalid.validate().is_err());

        // Invalid: context_percent out of range
        invalid = HandoffV2::new("session-1", "stage-1").with_context_percent(150.0);
        assert!(invalid.validate().is_err());
    }
}
