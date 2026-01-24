//! Checkpoint command implementation
//!
//! Usage: loom checkpoint <task-id> --status <status> [--force] [--output key=value]

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::checkpoints::{Checkpoint, CheckpointStatus};
use crate::commands::common::find_work_dir;
use crate::fs::checkpoints::{checkpoint_exists, write_checkpoint};
use crate::fs::task_state::write_task_state;
use crate::verify::task_verification::{run_task_verifications, summarize_verifications};

/// Execute the checkpoint command
///
/// Creates a checkpoint file signaling task completion.
/// Runs verification rules and reports warnings (soft failures).
pub fn execute(
    task_id: String,
    status: CheckpointStatus,
    force: bool,
    outputs: Vec<String>,
    notes: Option<String>,
) -> Result<()> {
    // Find the .work directory (in current dir or parent)
    let work_dir = find_work_dir()?;

    // Get session ID from environment or infer from context
    let session_id = get_current_session_id(&work_dir)?;

    // Parse outputs into HashMap
    let outputs_map = parse_outputs(&outputs)?;

    // Check if checkpoint already exists
    if checkpoint_exists(&work_dir, &session_id, &task_id) && !force {
        bail!("Checkpoint for task '{task_id}' already exists. Use --force to overwrite.");
    }

    // Get the stage ID from the session
    let stage_id = get_stage_id_from_session(&work_dir, &session_id)?;

    // Load task state if it exists
    let task_state = match crate::fs::task_state::read_task_state_if_exists(&work_dir, &stage_id)? {
        Some(state) => state,
        None => {
            // No task state - just create the checkpoint without verification
            println!("Warning: No task state found for stage '{stage_id}'. Creating checkpoint without verification.");
            return create_checkpoint_only(
                &work_dir,
                &session_id,
                &task_id,
                status,
                outputs_map,
                notes,
            );
        }
    };

    // Find the task definition
    let task_def = task_state.tasks.iter().find(|t| t.id == task_id);

    // Run verifications if task has them
    let verification_warnings = if let Some(task) = task_def {
        if !task.verification.is_empty() {
            let worktree_path = get_worktree_path(&work_dir, &stage_id)?;
            let results = run_task_verifications(&task.verification, &worktree_path, &outputs_map);
            let (passed, failed, warnings) = summarize_verifications(&results);

            if failed > 0 {
                println!(
                    "\n⚠️  Verification warnings ({failed} of {} checks failed):",
                    passed + failed
                );
                for warning in &warnings {
                    println!("   - {warning}");
                }

                if !force {
                    println!(
                        "\nCheckpoint created with warnings. Use --force to suppress this message."
                    );
                }
            } else {
                println!("✓ All {passed} verification checks passed");
            }

            warnings
        } else {
            Vec::new()
        }
    } else {
        println!(
            "Warning: Task '{task_id}' not found in task definitions. Creating checkpoint anyway."
        );
        Vec::new()
    };

    // Create the checkpoint
    let checkpoint =
        Checkpoint::new(task_id.clone(), status.clone()).with_outputs(outputs_map.clone());
    let checkpoint = if let Some(notes) = notes.clone() {
        checkpoint.with_notes(notes)
    } else {
        checkpoint
    };

    let checkpoint_path = write_checkpoint(&work_dir, &session_id, &checkpoint)?;
    println!("✓ Checkpoint created: {}", checkpoint_path.display());

    // Update task state
    let mut updated_state = task_state;
    updated_state.complete_task(
        &task_id,
        status.clone(),
        verification_warnings,
        force,
        outputs_map,
    );

    write_task_state(&work_dir, &updated_state)?;

    // Report next available tasks
    let available = updated_state.get_available_tasks();
    if !available.is_empty() {
        println!("\n📋 Next available tasks:");
        for task in available {
            println!("   - {} : {}", task.id, task.instruction);
        }
    } else if updated_state.all_tasks_completed() {
        println!("\n🎉 All tasks completed! Ready to run stage acceptance criteria.");
    }

    Ok(())
}

/// Create a checkpoint without task state tracking
fn create_checkpoint_only(
    work_dir: &Path,
    session_id: &str,
    task_id: &str,
    status: CheckpointStatus,
    outputs: HashMap<String, String>,
    notes: Option<String>,
) -> Result<()> {
    let checkpoint = Checkpoint::new(task_id.to_string(), status).with_outputs(outputs);
    let checkpoint = if let Some(notes) = notes {
        checkpoint.with_notes(notes)
    } else {
        checkpoint
    };

    let checkpoint_path = write_checkpoint(work_dir, session_id, &checkpoint)?;
    println!("✓ Checkpoint created: {}", checkpoint_path.display());
    Ok(())
}

/// Get the current session ID from environment or context
fn get_current_session_id(work_dir: &Path) -> Result<String> {
    // First check environment variable
    if let Ok(session_id) = std::env::var("LOOM_SESSION_ID") {
        return Ok(session_id);
    }

    // Try to infer from signal files in worktree
    let signals_dir = work_dir.join("signals");
    if signals_dir.exists() {
        let entries = std::fs::read_dir(&signals_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.starts_with("session-") {
                        return Ok(stem.to_string());
                    }
                }
            }
        }
    }

    bail!("Could not determine session ID. Set LOOM_SESSION_ID or run from within a loom session.")
}

/// Get the stage ID from a session file
fn get_stage_id_from_session(work_dir: &Path, session_id: &str) -> Result<String> {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));

    if !session_path.exists() {
        // Try to get from signal file
        let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
        if signal_path.exists() {
            let content = std::fs::read_to_string(&signal_path)?;
            // Parse stage from signal file
            for line in content.lines() {
                if let Some(stage) = line.strip_prefix("- **Stage**: ") {
                    return Ok(stage.trim().to_string());
                }
            }
        }
        bail!("Could not find session or signal file for {session_id}");
    }

    // Parse stage_id from session file YAML frontmatter
    let content = std::fs::read_to_string(&session_path)?;
    let session: crate::models::session::Session =
        crate::parser::frontmatter::parse_from_markdown(&content, "Session")?;

    session
        .stage_id
        .ok_or_else(|| anyhow::anyhow!("Session {session_id} has no associated stage"))
}

/// Get the worktree path for a stage
fn get_worktree_path(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    let stage_path = work_dir.join("stages").join(format!("{stage_id}.md"));

    if !stage_path.exists() {
        // Default to current directory
        return Ok(std::env::current_dir()?);
    }

    let content = std::fs::read_to_string(&stage_path)?;
    let stage: crate::models::stage::Stage =
        crate::parser::frontmatter::parse_from_markdown(&content, "Stage")?;

    if let Some(worktree) = stage.worktree {
        // Worktree path is relative to project root (parent of .work)
        let project_root = work_dir.parent().unwrap_or(work_dir);
        Ok(project_root.join(worktree))
    } else {
        // No worktree - use current directory
        Ok(std::env::current_dir()?)
    }
}

/// Parse output key=value pairs
fn parse_outputs(outputs: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();

    for output in outputs {
        let parts: Vec<&str> = output.splitn(2, '=').collect();
        if parts.len() != 2 {
            bail!("Invalid output format: '{output}'. Expected 'key=value'");
        }
        map.insert(parts[0].to_string(), parts[1].to_string());
    }

    Ok(map)
}

/// List checkpoints for the current session
pub fn list(session_id: Option<String>) -> Result<()> {
    let work_dir = find_work_dir()?;
    let session_id = match session_id {
        Some(id) => id,
        None => get_current_session_id(&work_dir)?,
    };

    let checkpoints = crate::fs::checkpoints::list_checkpoints(&work_dir, &session_id)?;

    if checkpoints.is_empty() {
        println!("No checkpoints found for session {session_id}");
        return Ok(());
    }

    println!("Checkpoints for session {session_id}:\n");
    for checkpoint in checkpoints {
        println!(
            "  {} : {} ({})",
            checkpoint.task_id,
            checkpoint.status,
            checkpoint.created_at.format("%Y-%m-%d %H:%M:%S")
        );
        if let Some(notes) = &checkpoint.notes {
            println!("    Notes: {notes}");
        }
        if !checkpoint.outputs.is_empty() {
            println!("    Outputs:");
            for (key, value) in &checkpoint.outputs {
                println!("      {key}: {value}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_outputs() {
        let outputs = vec![
            "key1=value1".to_string(),
            "key2=value with spaces".to_string(),
        ];
        let map = parse_outputs(&outputs).unwrap();

        assert_eq!(map.get("key1"), Some(&"value1".to_string()));
        assert_eq!(map.get("key2"), Some(&"value with spaces".to_string()));
    }

    #[test]
    fn test_parse_outputs_invalid() {
        let outputs = vec!["invalid".to_string()];
        let result = parse_outputs(&outputs);
        assert!(result.is_err());
    }
}
