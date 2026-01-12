//! Checkpoint types for task-level progress tracking
//!
//! Checkpoints enable incremental task verification within a stage.
//! Agents signal task completion via checkpoint files, and loom verifies
//! each task before unlocking the next one.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a checkpoint/task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointStatus {
    /// Task completed successfully
    Completed,
    /// Task is blocked and needs help
    Blocked,
    /// Task needs assistance but isn't fully blocked
    NeedsHelp,
}

impl std::fmt::Display for CheckpointStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointStatus::Completed => write!(f, "completed"),
            CheckpointStatus::Blocked => write!(f, "blocked"),
            CheckpointStatus::NeedsHelp => write!(f, "needs_help"),
        }
    }
}

impl std::str::FromStr for CheckpointStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "completed" => Ok(CheckpointStatus::Completed),
            "blocked" => Ok(CheckpointStatus::Blocked),
            "needs_help" | "needs-help" | "needshelp" => Ok(CheckpointStatus::NeedsHelp),
            _ => anyhow::bail!(
                "Invalid checkpoint status: {s}. Valid values: completed, blocked, needs_help"
            ),
        }
    }
}

/// A checkpoint file created by an agent to signal task completion
///
/// File location: `.work/checkpoints/{session-id}/{task-id}.yaml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique task identifier within the stage
    pub task_id: String,
    /// Current status of the task
    pub status: CheckpointStatus,
    /// Key-value outputs produced by the task (e.g., file paths, values)
    #[serde(default)]
    pub outputs: HashMap<String, String>,
    /// Optional notes from the agent about the task
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,
}

impl Checkpoint {
    pub fn new(task_id: String, status: CheckpointStatus) -> Self {
        Self {
            task_id,
            status,
            outputs: HashMap::new(),
            notes: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_outputs(mut self, outputs: HashMap<String, String>) -> Self {
        self.outputs = outputs;
        self
    }

    pub fn with_notes(mut self, notes: String) -> Self {
        self.notes = Some(notes);
        self
    }
}

/// A task definition within a stage
///
/// Tasks are defined in the stage description using YAML and have
/// verification rules that are checked when a checkpoint is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Unique task identifier
    pub id: String,
    /// Human-readable instruction for the task
    pub instruction: String,
    /// Verification rules to check on completion
    #[serde(default)]
    pub verification: Vec<VerificationRule>,
    /// Task IDs that must be completed before this task
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// A verification rule for a task
///
/// Verification rules are soft checks - they emit warnings but don't
/// hard-block progression. Agents can override with `--force`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VerificationRule {
    /// Check that a file exists
    FileExists {
        /// Path to the file (relative to worktree)
        path: String,
    },
    /// Check that a file contains a pattern (regex)
    Contains {
        /// Path to the file (relative to worktree)
        path: String,
        /// Regex pattern to match
        pattern: String,
    },
    /// Run a command and check exit code
    Command {
        /// Command to run
        cmd: String,
        /// Expected exit code (default: 0)
        #[serde(default)]
        expected_exit_code: i32,
    },
    /// Check that loom stage output was called
    OutputSet {
        /// Output key that should be set
        key: String,
    },
}

/// Result of running a verification rule
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub rule: VerificationRule,
    pub passed: bool,
    pub message: String,
}

impl VerificationResult {
    pub fn passed(rule: VerificationRule, message: impl Into<String>) -> Self {
        Self {
            rule,
            passed: true,
            message: message.into(),
        }
    }

    pub fn failed(rule: VerificationRule, message: impl Into<String>) -> Self {
        Self {
            rule,
            passed: false,
            message: message.into(),
        }
    }
}

/// Task state tracking for a stage
///
/// File location: `.work/task-state/{stage-id}.yaml`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskState {
    /// Stage ID this state belongs to
    pub stage_id: String,
    /// All task definitions for this stage
    #[serde(default)]
    pub tasks: Vec<TaskDefinition>,
    /// Current task being worked on (index into tasks)
    #[serde(default)]
    pub current_task_index: usize,
    /// Map of task_id -> completion status
    #[serde(default)]
    pub completed_tasks: HashMap<String, TaskCompletionRecord>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Record of a completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletionRecord {
    /// Final status of the task
    pub status: CheckpointStatus,
    /// Verification results (warnings if any failed)
    #[serde(default)]
    pub verification_warnings: Vec<String>,
    /// Whether the task was force-completed (bypassing verification)
    #[serde(default)]
    pub force_completed: bool,
    /// Outputs from the checkpoint
    #[serde(default)]
    pub outputs: HashMap<String, String>,
    /// Completion timestamp
    pub completed_at: DateTime<Utc>,
}

impl TaskState {
    pub fn new(stage_id: String, tasks: Vec<TaskDefinition>) -> Self {
        Self {
            stage_id,
            tasks,
            current_task_index: 0,
            completed_tasks: HashMap::new(),
            updated_at: Utc::now(),
        }
    }

    /// Get the current task (if any)
    pub fn current_task(&self) -> Option<&TaskDefinition> {
        self.tasks.get(self.current_task_index)
    }

    /// Check if a task is unlocked (all dependencies satisfied)
    pub fn is_task_unlocked(&self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter().find(|t| t.id == task_id) {
            task.depends_on
                .iter()
                .all(|dep| self.completed_tasks.contains_key(dep))
        } else {
            false
        }
    }

    /// Get list of unlocked but not completed tasks
    pub fn get_available_tasks(&self) -> Vec<&TaskDefinition> {
        self.tasks
            .iter()
            .filter(|t| !self.completed_tasks.contains_key(&t.id) && self.is_task_unlocked(&t.id))
            .collect()
    }

    /// Mark a task as complete and advance to next available task
    pub fn complete_task(
        &mut self,
        task_id: &str,
        status: CheckpointStatus,
        verification_warnings: Vec<String>,
        force_completed: bool,
        outputs: HashMap<String, String>,
    ) {
        self.completed_tasks.insert(
            task_id.to_string(),
            TaskCompletionRecord {
                status,
                verification_warnings,
                force_completed,
                outputs,
                completed_at: Utc::now(),
            },
        );

        // Advance current_task_index to next available task
        if let Some(idx) = self
            .tasks
            .iter()
            .position(|t| !self.completed_tasks.contains_key(&t.id) && self.is_task_unlocked(&t.id))
        {
            self.current_task_index = idx;
        }

        self.updated_at = Utc::now();
    }

    /// Check if all tasks are completed
    pub fn all_tasks_completed(&self) -> bool {
        self.tasks
            .iter()
            .all(|t| self.completed_tasks.contains_key(&t.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_status_parsing() {
        assert_eq!(
            "completed".parse::<CheckpointStatus>().unwrap(),
            CheckpointStatus::Completed
        );
        assert_eq!(
            "blocked".parse::<CheckpointStatus>().unwrap(),
            CheckpointStatus::Blocked
        );
        assert_eq!(
            "needs_help".parse::<CheckpointStatus>().unwrap(),
            CheckpointStatus::NeedsHelp
        );
        assert_eq!(
            "needs-help".parse::<CheckpointStatus>().unwrap(),
            CheckpointStatus::NeedsHelp
        );
    }

    #[test]
    fn test_task_state_progression() {
        let tasks = vec![
            TaskDefinition {
                id: "task-1".to_string(),
                instruction: "First task".to_string(),
                verification: vec![],
                depends_on: vec![],
            },
            TaskDefinition {
                id: "task-2".to_string(),
                instruction: "Second task".to_string(),
                verification: vec![],
                depends_on: vec!["task-1".to_string()],
            },
            TaskDefinition {
                id: "task-3".to_string(),
                instruction: "Third task".to_string(),
                verification: vec![],
                depends_on: vec!["task-2".to_string()],
            },
        ];

        let mut state = TaskState::new("test-stage".to_string(), tasks);

        // Initially only task-1 is available
        assert!(state.is_task_unlocked("task-1"));
        assert!(!state.is_task_unlocked("task-2"));
        assert!(!state.is_task_unlocked("task-3"));

        // Complete task-1
        state.complete_task(
            "task-1",
            CheckpointStatus::Completed,
            vec![],
            false,
            HashMap::new(),
        );

        // Now task-2 is unlocked
        assert!(state.is_task_unlocked("task-2"));
        assert!(!state.is_task_unlocked("task-3"));

        // Complete task-2
        state.complete_task(
            "task-2",
            CheckpointStatus::Completed,
            vec![],
            false,
            HashMap::new(),
        );

        // Now task-3 is unlocked
        assert!(state.is_task_unlocked("task-3"));
        assert!(!state.all_tasks_completed());

        // Complete task-3
        state.complete_task(
            "task-3",
            CheckpointStatus::Completed,
            vec![],
            false,
            HashMap::new(),
        );

        assert!(state.all_tasks_completed());
    }
}
