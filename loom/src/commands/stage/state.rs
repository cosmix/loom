//! Stage state transition commands

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::session_files::find_session_file;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::backend::SessionBackend;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::{load_stage, update_stage};

/// Block a stage with a reason
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    update_stage(&stage_id, work_dir, |stage| {
        stage.try_mark_blocked()?;
        stage.close_reason = Some(reason.clone());
        stage.updated_at = chrono::Utc::now();
        Ok(())
    })?;

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Reset a stage to pending
///
/// NOTE: This is a manual recovery command that intentionally bypasses state machine validation.
/// WaitingForDeps has no incoming transitions because it's the initial state. For recovery scenarios,
/// we allow direct assignment to reset stages to their initial state.
pub fn reset(stage_id: String, hard: bool, kill_session: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let stage = load_stage(&stage_id, work_dir)?;

    // Kill the associated session before resetting, if requested. This prevents
    // a duplicate-session hazard where the old session keeps running while the
    // respawned stage starts a new one.
    if kill_session {
        if let Some(ref session_id) = stage.session.clone() {
            let kill_result = find_session_file(work_dir, session_id)
                .context("Failed to locate session file")
                .and_then(|maybe_path| match maybe_path {
                    None => {
                        eprintln!("Note: No session file found for '{session_id}', skipping kill");
                        Ok(())
                    }
                    Some(session_file) => std::fs::read_to_string(&session_file)
                        .context("Failed to read session file")
                        .and_then(|content| {
                            parse_from_markdown::<Session>(&content, "Session")
                                .context("Failed to parse session")
                        })
                        .and_then(|session| {
                            SessionBackend::from_config(work_dir.to_path_buf())
                                .context("Failed to construct session backend")
                                .and_then(|backend| {
                                    if backend.is_session_alive(&session)? {
                                        backend.kill_session(&session)?;
                                        println!("  Killed session '{session_id}'");
                                    } else {
                                        println!("  Session '{session_id}' already terminated");
                                    }
                                    Ok(())
                                })
                        }),
                });
            if let Err(e) = kill_result {
                eprintln!("Warning: Failed to kill session '{session_id}': {e}");
            }
        } else {
            eprintln!("Note: Stage '{stage_id}' has no associated session to kill");
        }
    }

    // INTENTIONAL STATE MACHINE BYPASS: WaitingForDeps is the initial state and
    // has no valid incoming transitions. Apply only reset-owned fields to the
    // fresh record under lock so unrelated concurrent changes survive.
    eprintln!(
        "Warning: Bypassing state machine to reset stage to initial state (was: {:?})",
        stage.status
    );
    update_stage(&stage_id, work_dir, |current| {
        apply_reset(current, hard);
        Ok(())
    })?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut skipped_status = None;
    update_stage(&stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Executing {
            skipped_status = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_waiting_for_input()
    })?;
    if let Some(status) = skipped_status {
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, status
        );
        return Ok(());
    }

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut skipped_status = None;
    update_stage(&stage_id, work_dir, |stage| {
        if stage.status != StageStatus::WaitingForInput {
            skipped_status = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_executing()
    })?;
    if let Some(status) = skipped_status {
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, status
        );
        return Ok(());
    }

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}

/// Hold a stage (prevent auto-execution even when ready)
pub fn hold(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut already_held = false;
    update_stage(&stage_id, work_dir, |stage| {
        if stage.held {
            already_held = true;
        } else {
            stage.hold();
        }
        Ok(())
    })?;
    if already_held {
        println!("Stage '{stage_id}' is already held");
        return Ok(());
    }

    println!("Stage '{stage_id}' held");
    println!("The stage will not auto-execute. Use 'loom stage release {stage_id}' to unlock.");
    Ok(())
}

/// Release a held stage (allow auto-execution)
pub fn release(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut already_released = false;
    update_stage(&stage_id, work_dir, |stage| {
        if !stage.held {
            already_released = true;
        } else {
            stage.release();
        }
        Ok(())
    })?;
    if already_released {
        println!("Stage '{stage_id}' is not held");
        return Ok(());
    }

    println!("Stage '{stage_id}' released");
    Ok(())
}

fn apply_reset(stage: &mut Stage, hard: bool) {
    stage.status = StageStatus::WaitingForDeps;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.started_at = None;
    stage.duration_secs = None;
    stage.retry_count = 0;
    stage.fix_attempts = 0;
    stage.last_failure_at = None;
    stage.failure_info = None;
    stage.updated_at = chrono::Utc::now();
    if hard {
        stage.session = None;
    }
}
