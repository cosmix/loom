//! Stage state transition commands

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::session_files::find_session_file;
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::NativeBackend;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::{load_stage, save_stage};

/// Block a stage with a reason
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.try_mark_blocked()?;
    stage.close_reason = Some(reason.clone());
    stage.updated_at = chrono::Utc::now();
    save_stage(&stage, work_dir)?;

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

    let mut stage = load_stage(&stage_id, work_dir)?;

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
                            NativeBackend::new(work_dir.to_path_buf())
                                .context("Failed to construct native backend")
                                .and_then(|native| {
                                    if native.is_session_alive(&session)? {
                                        native.kill_session(&session)?;
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

    // INTENTIONAL STATE MACHINE BYPASS: WaitingForDeps is the initial state
    // and has no valid incoming transitions. This manual recovery command
    // allows resetting stages to their initial state for recovery scenarios.
    eprintln!(
        "Warning: Bypassing state machine to reset stage to initial state (was: {:?})",
        stage.status
    );
    stage.status = StageStatus::WaitingForDeps;

    // Clear completion state
    stage.completed_at = None;
    stage.close_reason = None;

    // Clear timing fields
    stage.started_at = None;
    stage.duration_secs = None;

    // Clear retry state
    stage.retry_count = 0;
    stage.fix_attempts = 0;
    stage.last_failure_at = None;
    stage.failure_info = None;

    stage.updated_at = chrono::Utc::now();

    // Hard reset also clears the session assignment on the stage record.
    // Note: this does NOT run `git reset --hard` in the worktree; if you
    // need to clean the worktree, run that manually before this command.
    if hard {
        stage.session = None;
    }

    save_stage(&stage, work_dir)?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as ready for execution
///
/// Note: This is an internal function not exposed via CLI. The orchestrator
/// handles WaitingForDeps -> Queued transitions automatically.
#[allow(dead_code)]
pub fn ready(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.try_mark_queued()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as ready");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently executing
    if stage.status != StageStatus::Executing {
        // Silently skip if not executing - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_waiting_for_input()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently waiting for input
    if stage.status != StageStatus::WaitingForInput {
        // Silently skip if not waiting - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_executing()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}

/// Hold a stage (prevent auto-execution even when ready)
pub fn hold(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if stage.held {
        println!("Stage '{stage_id}' is already held");
        return Ok(());
    }

    stage.hold();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' held");
    println!("The stage will not auto-execute. Use 'loom stage release {stage_id}' to unlock.");
    Ok(())
}

/// Release a held stage (allow auto-execution)
pub fn release(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if !stage.held {
        println!("Stage '{stage_id}' is not held");
        return Ok(());
    }

    stage.release();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' released");
    Ok(())
}
