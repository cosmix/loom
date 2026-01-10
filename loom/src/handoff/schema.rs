//! Structured handoff schema for validated YAML handoffs.
//!
//! This module defines the V2 handoff format which uses typed YAML fields
//! instead of unstructured prose. This enables machine-readable handoffs
//! that can be validated and parsed reliably.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Version of the handoff schema
pub const HANDOFF_SCHEMA_VERSION: u32 = 2;

/// A reference to a file with optional line range and purpose description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    /// Path to the file (relative to project root)
    pub path: String,
    /// Optional line range (start, end) - both inclusive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<(u32, u32)>,
    /// Purpose or reason for referencing this file
    pub purpose: String,
}

impl FileRef {
    /// Create a new FileRef without line range
    pub fn new(path: impl Into<String>, purpose: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            lines: None,
            purpose: purpose.into(),
        }
    }

    /// Create a new FileRef with a line range
    pub fn with_lines(
        path: impl Into<String>,
        start: u32,
        end: u32,
        purpose: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            lines: Some((start, end)),
            purpose: purpose.into(),
        }
    }

    /// Format as file:line reference string
    pub fn to_ref_string(&self) -> String {
        match self.lines {
            Some((start, end)) if start == end => format!("{}:{}", self.path, start),
            Some((start, end)) => format!("{}:{}-{}", self.path, start, end),
            None => self.path.clone(),
        }
    }
}

/// A completed task with description and optional file references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedTask {
    /// Description of what was accomplished
    pub description: String,
    /// Files involved in this task
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

impl CompletedTask {
    /// Create a new completed task
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            files: Vec::new(),
        }
    }

    /// Create a completed task with associated files
    pub fn with_files(description: impl Into<String>, files: Vec<String>) -> Self {
        Self {
            description: description.into(),
            files,
        }
    }
}

/// A key decision made during the session with rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyDecision {
    /// The decision that was made
    pub decision: String,
    /// Rationale for the decision
    pub rationale: String,
}

impl KeyDecision {
    pub fn new(decision: impl Into<String>, rationale: impl Into<String>) -> Self {
        Self {
            decision: decision.into(),
            rationale: rationale.into(),
        }
    }
}

/// A commit made during the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitRef {
    /// Short commit hash (7-8 characters)
    pub hash: String,
    /// Commit message (first line)
    pub message: String,
}

impl CommitRef {
    pub fn new(hash: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            message: message.into(),
        }
    }
}

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

    /// Set discovered facts
    pub fn with_discovered_facts(mut self, facts: Vec<String>) -> Self {
        self.discovered_facts = facts;
        self
    }

    /// Set open questions
    pub fn with_open_questions(mut self, questions: Vec<String>) -> Self {
        self.open_questions = questions;
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
        let handoff: Self =
            serde_yaml::from_str(yaml).context("Failed to parse handoff YAML")?;
        handoff.validate()?;
        Ok(handoff)
    }

    /// Validate the handoff data
    pub fn validate(&self) -> Result<()> {
        if self.version != HANDOFF_SCHEMA_VERSION {
            anyhow::bail!(
                "Unsupported handoff version: {}. Expected {}.",
                self.version,
                HANDOFF_SCHEMA_VERSION
            );
        }

        if self.session_id.is_empty() {
            anyhow::bail!("Handoff session_id cannot be empty");
        }

        if self.stage_id.is_empty() {
            anyhow::bail!("Handoff stage_id cannot be empty");
        }

        if !(0.0..=100.0).contains(&self.context_percent) {
            anyhow::bail!(
                "Handoff context_percent must be between 0.0 and 100.0, got {}",
                self.context_percent
            );
        }

        Ok(())
    }
}

/// Result of attempting to parse a handoff file.
///
/// Supports both V2 (structured YAML) and V1 (prose markdown) formats.
#[derive(Debug)]
pub enum ParsedHandoff {
    /// Successfully parsed V2 structured handoff
    V2(HandoffV2),
    /// Fell back to V1 prose format (raw markdown content)
    V1Fallback(String),
}

impl ParsedHandoff {
    /// Try to parse a handoff file, falling back to V1 if V2 parsing fails
    pub fn parse(content: &str) -> Self {
        // Try to extract YAML frontmatter or find YAML block
        if let Some(yaml_content) = extract_yaml_from_handoff(content) {
            if let Ok(handoff) = HandoffV2::from_yaml(&yaml_content) {
                return ParsedHandoff::V2(handoff);
            }
        }

        // Fall back to V1 prose format
        ParsedHandoff::V1Fallback(content.to_string())
    }

    /// Check if this is a V2 handoff
    pub fn is_v2(&self) -> bool {
        matches!(self, ParsedHandoff::V2(_))
    }

    /// Get V2 handoff if available
    pub fn as_v2(&self) -> Option<&HandoffV2> {
        match self {
            ParsedHandoff::V2(h) => Some(h),
            ParsedHandoff::V1Fallback(_) => None,
        }
    }

    /// Get raw content (V1) if this is a fallback
    pub fn as_v1(&self) -> Option<&str> {
        match self {
            ParsedHandoff::V2(_) => None,
            ParsedHandoff::V1Fallback(content) => Some(content),
        }
    }
}

/// Extract YAML content from a handoff file.
///
/// Supports two formats:
/// 1. YAML frontmatter (content between --- markers at start of file)
/// 2. Pure YAML (file starts with "version:")
fn extract_yaml_from_handoff(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // Check for YAML frontmatter
    if trimmed.starts_with("---") {
        let after_first = &trimmed[3..];
        if let Some(end_idx) = after_first.find("---") {
            let yaml = after_first[..end_idx].trim();
            return Some(yaml.to_string());
        }
    }

    // Check if the file is pure YAML (starts with version:)
    if trimmed.starts_with("version:") {
        return Some(trimmed.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_ref_new() {
        let fr = FileRef::new("src/main.rs", "entry point");
        assert_eq!(fr.path, "src/main.rs");
        assert_eq!(fr.lines, None);
        assert_eq!(fr.purpose, "entry point");
    }

    #[test]
    fn test_file_ref_with_lines() {
        let fr = FileRef::with_lines("src/lib.rs", 10, 50, "module definition");
        assert_eq!(fr.path, "src/lib.rs");
        assert_eq!(fr.lines, Some((10, 50)));
        assert_eq!(fr.purpose, "module definition");
    }

    #[test]
    fn test_file_ref_to_ref_string() {
        assert_eq!(
            FileRef::new("src/main.rs", "").to_ref_string(),
            "src/main.rs"
        );
        assert_eq!(
            FileRef::with_lines("src/lib.rs", 10, 10, "").to_ref_string(),
            "src/lib.rs:10"
        );
        assert_eq!(
            FileRef::with_lines("src/lib.rs", 10, 50, "").to_ref_string(),
            "src/lib.rs:10-50"
        );
    }

    #[test]
    fn test_completed_task() {
        let task = CompletedTask::new("Implemented feature X");
        assert_eq!(task.description, "Implemented feature X");
        assert!(task.files.is_empty());

        let task_with_files =
            CompletedTask::with_files("Fixed bug", vec!["src/bug.rs".to_string()]);
        assert_eq!(task_with_files.files.len(), 1);
    }

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
            .with_key_decisions(vec![KeyDecision::new("Used pattern X", "Better performance")])
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

    #[test]
    fn test_parsed_handoff_v2() {
        let yaml = r#"
version: 2
session_id: session-test
stage_id: stage-test
context_percent: 70.0
"#;
        let parsed = ParsedHandoff::parse(yaml);
        assert!(parsed.is_v2());
        let v2 = parsed.as_v2().unwrap();
        assert_eq!(v2.session_id, "session-test");
    }

    #[test]
    fn test_parsed_handoff_with_frontmatter() {
        let content = r#"---
version: 2
session_id: session-fm
stage_id: stage-fm
context_percent: 80.0
---

# Additional markdown content
"#;
        let parsed = ParsedHandoff::parse(content);
        assert!(parsed.is_v2());
        let v2 = parsed.as_v2().unwrap();
        assert_eq!(v2.session_id, "session-fm");
    }

    #[test]
    fn test_parsed_handoff_v1_fallback() {
        let content = "# Handoff: my-stage\n\n## Completed Work\n\n- Did stuff";
        let parsed = ParsedHandoff::parse(content);
        assert!(!parsed.is_v2());
        assert!(parsed.as_v1().is_some());
    }

    #[test]
    fn test_extract_yaml_from_handoff_frontmatter() {
        let content = "---\nversion: 2\n---\n# Rest of file";
        let yaml = extract_yaml_from_handoff(content).unwrap();
        assert_eq!(yaml, "version: 2");
    }

    #[test]
    fn test_extract_yaml_from_handoff_pure_yaml() {
        let content = "version: 2\nsession_id: test";
        let yaml = extract_yaml_from_handoff(content).unwrap();
        assert!(yaml.contains("version: 2"));
    }

    #[test]
    fn test_extract_yaml_from_handoff_prose() {
        let content = "# Handoff\n\nSome prose content";
        assert!(extract_yaml_from_handoff(content).is_none());
    }
}
