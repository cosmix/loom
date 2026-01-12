//! Checkpoint file I/O operations
//!
//! Handles reading and writing checkpoint files at `.work/checkpoints/{session-id}/{task-id}.yaml`

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::checkpoints::{Checkpoint, CheckpointStatus};

/// Get the checkpoints directory for a session
pub fn checkpoints_dir(work_dir: &Path, session_id: &str) -> PathBuf {
    work_dir.join("checkpoints").join(session_id)
}

/// Get the path to a specific checkpoint file
pub fn checkpoint_path(work_dir: &Path, session_id: &str, task_id: &str) -> PathBuf {
    checkpoints_dir(work_dir, session_id).join(format!("{task_id}.yaml"))
}

/// Ensure the checkpoints directory exists for a session
pub fn ensure_checkpoints_dir(work_dir: &Path, session_id: &str) -> Result<PathBuf> {
    let dir = checkpoints_dir(work_dir, session_id);
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| {
            format!("Failed to create checkpoints directory: {}", dir.display())
        })?;
    }
    Ok(dir)
}

/// Write a checkpoint file
pub fn write_checkpoint(
    work_dir: &Path,
    session_id: &str,
    checkpoint: &Checkpoint,
) -> Result<PathBuf> {
    ensure_checkpoints_dir(work_dir, session_id)?;
    let path = checkpoint_path(work_dir, session_id, &checkpoint.task_id);

    let yaml =
        serde_yaml::to_string(checkpoint).context("Failed to serialize checkpoint to YAML")?;

    fs::write(&path, yaml)
        .with_context(|| format!("Failed to write checkpoint file: {}", path.display()))?;

    Ok(path)
}

/// Read a checkpoint file
pub fn read_checkpoint(work_dir: &Path, session_id: &str, task_id: &str) -> Result<Checkpoint> {
    let path = checkpoint_path(work_dir, session_id, task_id);

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read checkpoint file: {}", path.display()))?;

    let checkpoint: Checkpoint = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse checkpoint file: {}", path.display()))?;

    Ok(checkpoint)
}

/// List all checkpoints for a session
pub fn list_checkpoints(work_dir: &Path, session_id: &str) -> Result<Vec<Checkpoint>> {
    let dir = checkpoints_dir(work_dir, session_id);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut checkpoints = Vec::new();
    let entries = fs::read_dir(&dir)
        .with_context(|| format!("Failed to read checkpoints directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_yaml::from_str::<Checkpoint>(&content) {
                    Ok(checkpoint) => checkpoints.push(checkpoint),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse checkpoint {}: {}",
                            path.display(),
                            e
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read checkpoint {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(checkpoints)
}

/// Check if a checkpoint exists
pub fn checkpoint_exists(work_dir: &Path, session_id: &str, task_id: &str) -> bool {
    checkpoint_path(work_dir, session_id, task_id).exists()
}

/// Delete a checkpoint file
pub fn delete_checkpoint(work_dir: &Path, session_id: &str, task_id: &str) -> Result<()> {
    let path = checkpoint_path(work_dir, session_id, task_id);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete checkpoint: {}", path.display()))?;
    }
    Ok(())
}

/// Delete all checkpoints for a session
pub fn delete_session_checkpoints(work_dir: &Path, session_id: &str) -> Result<()> {
    let dir = checkpoints_dir(work_dir, session_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| {
            format!("Failed to delete checkpoints directory: {}", dir.display())
        })?;
    }
    Ok(())
}

/// Watch for new checkpoints (returns task IDs of new checkpoints since last check)
pub fn find_new_checkpoints(
    work_dir: &Path,
    session_id: &str,
    known_checkpoints: &[String],
) -> Result<Vec<Checkpoint>> {
    let checkpoints = list_checkpoints(work_dir, session_id)?;

    Ok(checkpoints
        .into_iter()
        .filter(|c| !known_checkpoints.contains(&c.task_id))
        .collect())
}

/// Create a checkpoint with the given status
pub fn create_checkpoint(
    work_dir: &Path,
    session_id: &str,
    task_id: &str,
    status: CheckpointStatus,
    notes: Option<String>,
) -> Result<PathBuf> {
    let mut checkpoint = Checkpoint::new(task_id.to_string(), status);
    if let Some(notes) = notes {
        checkpoint = checkpoint.with_notes(notes);
    }
    write_checkpoint(work_dir, session_id, &checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_checkpoint_roundtrip() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let checkpoint = Checkpoint::new("task-1".to_string(), CheckpointStatus::Completed)
            .with_notes("Test notes".to_string());

        write_checkpoint(work_dir, "session-123", &checkpoint).unwrap();

        let loaded = read_checkpoint(work_dir, "session-123", "task-1").unwrap();

        assert_eq!(loaded.task_id, "task-1");
        assert_eq!(loaded.status, CheckpointStatus::Completed);
        assert_eq!(loaded.notes, Some("Test notes".to_string()));
    }

    #[test]
    fn test_list_checkpoints() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path();

        let c1 = Checkpoint::new("task-1".to_string(), CheckpointStatus::Completed);
        let c2 = Checkpoint::new("task-2".to_string(), CheckpointStatus::Blocked);

        write_checkpoint(work_dir, "session-123", &c1).unwrap();
        write_checkpoint(work_dir, "session-123", &c2).unwrap();

        let checkpoints = list_checkpoints(work_dir, "session-123").unwrap();
        assert_eq!(checkpoints.len(), 2);
    }
}
