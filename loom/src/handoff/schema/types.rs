//! Core types for handoff schema.

use serde::{Deserialize, Serialize};

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
}
