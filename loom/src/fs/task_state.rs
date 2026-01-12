//! Task state file I/O operations
//!
//! Handles reading and writing task state files at `.work/task-state/{stage-id}.yaml`

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::checkpoints::TaskState;

/// Get the task-state directory
pub fn task_state_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("task-state")
}

/// Get the path to a specific task state file
pub fn task_state_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    task_state_dir(work_dir).join(format!("{stage_id}.yaml"))
}

/// Ensure the task-state directory exists
pub fn ensure_task_state_dir(work_dir: &Path) -> Result<PathBuf> {
    let dir = task_state_dir(work_dir);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create task-state directory: {}", dir.display()))?;
    }
    Ok(dir)
}

/// Write a task state file
pub fn write_task_state(work_dir: &Path, state: &TaskState) -> Result<PathBuf> {
    ensure_task_state_dir(work_dir)?;
    let path = task_state_path(work_dir, &state.stage_id);

    let yaml = serde_yaml::to_string(state).context("Failed to serialize task state to YAML")?;

    fs::write(&path, yaml)
        .with_context(|| format!("Failed to write task state file: {}", path.display()))?;

    Ok(path)
}

/// Read a task state file
pub fn read_task_state(work_dir: &Path, stage_id: &str) -> Result<TaskState> {
    let path = task_state_path(work_dir, stage_id);

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read task state file: {}", path.display()))?;

    let state: TaskState = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse task state file: {}", path.display()))?;

    Ok(state)
}

/// Read a task state file, or return None if it doesn't exist
pub fn read_task_state_if_exists(work_dir: &Path, stage_id: &str) -> Result<Option<TaskState>> {
    let path = task_state_path(work_dir, stage_id);
    if !path.exists() {
        return Ok(None);
    }
    read_task_state(work_dir, stage_id).map(Some)
}

/// Check if a task state file exists
pub fn task_state_exists(work_dir: &Path, stage_id: &str) -> bool {
    task_state_path(work_dir, stage_id).exists()
}

/// Delete a task state file
pub fn delete_task_state(work_dir: &Path, stage_id: &str) -> Result<()> {
    let path = task_state_path(work_dir, stage_id);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete task state: {}", path.display()))?;
    }
    Ok(())
}

/// List all task state files
pub fn list_task_states(work_dir: &Path) -> Result<Vec<TaskState>> {
    let dir = task_state_dir(work_dir);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut states = Vec::new();
    let entries = fs::read_dir(&dir)
        .with_context(|| format!("Failed to read task-state directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_yaml::from_str::<TaskState>(&content) {
                    Ok(state) => states.push(state),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse task state {}: {}",
                            path.display(),
                            e
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read task state {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoints::{CheckpointStatus, TaskDefinition};
    use tempfile::TempDir;

    #[test]
    fn test_task_state_roundtrip() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let tasks = vec![TaskDefinition {
            id: "task-1".to_string(),
            instruction: "Do something".to_string(),
            verification: vec![],
            depends_on: vec![],
        }];

        let mut state = TaskState::new("test-stage".to_string(), tasks);
        state.complete_task(
            "task-1",
            CheckpointStatus::Completed,
            vec![],
            false,
            std::collections::HashMap::new(),
        );

        write_task_state(work_dir, &state).unwrap();

        let loaded = read_task_state(work_dir, "test-stage").unwrap();

        assert_eq!(loaded.stage_id, "test-stage");
        assert!(loaded.completed_tasks.contains_key("task-1"));
    }
}
