//! Checkpoint watcher for the orchestrator monitor
//!
//! Detects new checkpoints and processes them:
//! - Runs task verification rules
//! - Updates task state
//! - Injects next task info or correction guidance into signals

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::checkpoints::Checkpoint;
use crate::fs::checkpoints::list_checkpoints;
use crate::fs::task_state::{read_task_state_if_exists, write_task_state};
use crate::verify::task_verification::{run_task_verifications, summarize_verifications};

/// Result of processing a checkpoint
#[derive(Debug, Clone)]
pub struct CheckpointProcessResult {
    pub session_id: String,
    pub task_id: String,
    pub verification_passed: bool,
    pub warnings: Vec<String>,
    pub next_tasks: Vec<NextTaskInfo>,
    pub stage_complete: bool,
}

/// Information about the next available task
#[derive(Debug, Clone)]
pub struct NextTaskInfo {
    pub id: String,
    pub instruction: String,
}

/// Checkpoint watcher state
pub struct CheckpointWatcher {
    /// Known checkpoints per session (session_id -> set of task_ids)
    known_checkpoints: HashMap<String, HashSet<String>>,
}

impl CheckpointWatcher {
    pub fn new() -> Self {
        Self {
            known_checkpoints: HashMap::new(),
        }
    }

    /// Poll for new checkpoints across all sessions
    pub fn poll(&mut self, work_dir: &Path) -> Vec<CheckpointProcessResult> {
        let mut results = Vec::new();

        // Get list of all session checkpoint directories
        let checkpoints_dir = work_dir.join("checkpoints");
        if !checkpoints_dir.exists() {
            return results;
        }

        let entries = match std::fs::read_dir(&checkpoints_dir) {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        // Collect new checkpoints first, then process them
        let mut new_checkpoints: Vec<(String, Checkpoint)> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let session_id = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Get known checkpoints for this session
            let known = self
                .known_checkpoints
                .entry(session_id.clone())
                .or_default();

            // List all checkpoints for this session
            let checkpoints = match list_checkpoints(work_dir, &session_id) {
                Ok(checkpoints) => checkpoints,
                Err(_) => continue,
            };

            // Collect new checkpoints
            for checkpoint in checkpoints {
                if !known.contains(&checkpoint.task_id) {
                    new_checkpoints.push((session_id.clone(), checkpoint));
                }
            }
        }

        // Process new checkpoints
        for (session_id, checkpoint) in new_checkpoints {
            if let Some(result) =
                Self::process_checkpoint_static(work_dir, &session_id, &checkpoint)
            {
                results.push(result);
            }
            self.known_checkpoints
                .entry(session_id)
                .or_default()
                .insert(checkpoint.task_id);
        }

        results
    }

    /// Process a single checkpoint (static version for borrow checker)
    fn process_checkpoint_static(
        work_dir: &Path,
        session_id: &str,
        checkpoint: &Checkpoint,
    ) -> Option<CheckpointProcessResult> {
        // Get the stage ID from the session
        let stage_id = get_stage_id_from_session(work_dir, session_id)?;

        // Load task state
        let mut task_state = match read_task_state_if_exists(work_dir, &stage_id) {
            Ok(Some(state)) => state,
            _ => return None,
        };

        // Find the task definition
        let task_def = task_state
            .tasks
            .iter()
            .find(|t| t.id == checkpoint.task_id)?;

        // Run verifications
        let worktree_path = get_worktree_path(work_dir, &stage_id)?;
        let verification_results =
            run_task_verifications(&task_def.verification, &worktree_path, &checkpoint.outputs);
        let (passed, _failed, warnings) = summarize_verifications(&verification_results);

        // Update task state
        let force_completed = false; // Checkpoints from file are never force-completed
        task_state.complete_task(
            &checkpoint.task_id,
            checkpoint.status.clone(),
            warnings.clone(),
            force_completed,
            checkpoint.outputs.clone(),
        );

        // Save updated task state
        if let Err(e) = write_task_state(work_dir, &task_state) {
            eprintln!("Warning: Failed to save task state: {e}");
        }

        // Get next available tasks
        let next_tasks: Vec<NextTaskInfo> = task_state
            .get_available_tasks()
            .iter()
            .map(|t| NextTaskInfo {
                id: t.id.clone(),
                instruction: t.instruction.clone(),
            })
            .collect();

        let verification_passed = passed == verification_results.len();
        let stage_complete = task_state.all_tasks_completed();

        Some(CheckpointProcessResult {
            session_id: session_id.to_string(),
            task_id: checkpoint.task_id.clone(),
            verification_passed,
            warnings,
            next_tasks,
            stage_complete,
        })
    }

    /// Reset known checkpoints (for testing or reinitialization)
    pub fn reset(&mut self) {
        self.known_checkpoints.clear();
    }

    /// Mark a checkpoint as known (to skip processing)
    pub fn mark_known(&mut self, session_id: &str, task_id: &str) {
        self.known_checkpoints
            .entry(session_id.to_string())
            .or_default()
            .insert(task_id.to_string());
    }
}

impl Default for CheckpointWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the stage ID from a session (helper function)
fn get_stage_id_from_session(work_dir: &Path, session_id: &str) -> Option<String> {
    // Try session file first
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));
    if session_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&session_path) {
            if let Ok(session) = crate::parser::frontmatter::parse_from_markdown::<
                crate::models::session::Session,
            >(&content, "Session")
            {
                return session.stage_id;
            }
        }
    }

    // Try signal file
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    if signal_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&signal_path) {
            for line in content.lines() {
                if let Some(stage) = line.strip_prefix("- **Stage**: ") {
                    return Some(stage.trim().to_string());
                }
            }
        }
    }

    None
}

/// Get the worktree path for a stage (helper function)
fn get_worktree_path(work_dir: &Path, stage_id: &str) -> Option<std::path::PathBuf> {
    let stage_path = work_dir.join("stages").join(format!("{stage_id}.md"));
    if !stage_path.exists() {
        // Default to project root
        return work_dir.parent().map(|p| p.to_path_buf());
    }

    let content = std::fs::read_to_string(&stage_path).ok()?;
    let stage: crate::models::stage::Stage =
        crate::parser::frontmatter::parse_from_markdown(&content, "Stage").ok()?;

    if let Some(worktree) = stage.worktree {
        let project_root = work_dir.parent()?;
        Some(project_root.join(worktree))
    } else {
        work_dir.parent().map(|p| p.to_path_buf())
    }
}

/// Generate correction guidance for failed verification
pub fn generate_correction_guidance(warnings: &[String]) -> String {
    let mut guidance = String::new();
    guidance.push_str("⚠️ **Verification Warnings**\n\n");
    guidance.push_str("The following verification checks did not pass:\n\n");

    for warning in warnings {
        guidance.push_str(&format!("- {warning}\n"));
    }

    guidance.push_str(
        "\nThese are soft warnings - you may continue, but consider addressing these issues.\n",
    );

    guidance
}

/// Generate next task injection for signals
pub fn generate_next_task_injection(next_tasks: &[NextTaskInfo]) -> String {
    let mut content = String::new();

    if next_tasks.is_empty() {
        content.push_str("🎉 All tasks completed! Run stage acceptance criteria.\n");
    } else {
        content.push_str("📋 **Next Available Tasks:**\n\n");
        for task in next_tasks {
            content.push_str(&format!("- **{}**: {}\n", task.id, task.instruction));
        }
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_watcher_new() {
        let watcher = CheckpointWatcher::new();
        assert!(watcher.known_checkpoints.is_empty());
    }

    #[test]
    fn test_mark_known() {
        let mut watcher = CheckpointWatcher::new();
        watcher.mark_known("session-1", "task-1");

        assert!(watcher.known_checkpoints.contains_key("session-1"));
        assert!(watcher
            .known_checkpoints
            .get("session-1")
            .unwrap()
            .contains("task-1"));
    }

    #[test]
    fn test_generate_correction_guidance() {
        let warnings = vec![
            "File not found: src/new.rs".to_string(),
            "Pattern 'impl Foo' not found".to_string(),
        ];

        let guidance = generate_correction_guidance(&warnings);
        assert!(guidance.contains("Verification Warnings"));
        assert!(guidance.contains("File not found"));
        assert!(guidance.contains("Pattern 'impl Foo'"));
    }

    #[test]
    fn test_generate_next_task_injection_with_tasks() {
        let tasks = vec![
            NextTaskInfo {
                id: "task-2".to_string(),
                instruction: "Implement the feature".to_string(),
            },
            NextTaskInfo {
                id: "task-3".to_string(),
                instruction: "Add tests".to_string(),
            },
        ];

        let injection = generate_next_task_injection(&tasks);
        assert!(injection.contains("Next Available Tasks"));
        assert!(injection.contains("task-2"));
        assert!(injection.contains("Implement the feature"));
    }

    #[test]
    fn test_generate_next_task_injection_empty() {
        let tasks: Vec<NextTaskInfo> = vec![];
        let injection = generate_next_task_injection(&tasks);
        assert!(injection.contains("All tasks completed"));
    }
}
