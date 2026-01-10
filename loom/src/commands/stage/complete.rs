//! Stage completion logic

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::session::{
    cleanup_session_resources, detect_backend_from_session, find_session_for_stage, BackendType,
};

/// Mark a stage as complete, optionally running acceptance criteria.
/// If acceptance criteria pass, auto-verifies the stage and triggers dependents.
/// If --no-verify is used or criteria fail, marks as Completed for manual review.
pub fn complete(stage_id: String, session_id: Option<String>, no_verify: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree path from stage's worktree field
    let working_dir: Option<PathBuf> = stage
        .worktree
        .as_ref()
        .map(|w| PathBuf::from(".worktrees").join(w))
        .filter(|p| p.exists());

    // Track whether all acceptance criteria passed
    let mut all_passed = true;

    // Run acceptance criteria unless --no-verify is specified
    if !no_verify && !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        if let Some(ref dir) = working_dir {
            println!("  (working directory: {})", dir.display());
        }

        for criterion in &stage.acceptance {
            println!("  → {criterion}");
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(criterion);

            if let Some(ref dir) = working_dir {
                cmd.current_dir(dir);
            }

            let status = cmd
                .status()
                .with_context(|| format!("Failed to run: {criterion}"))?;

            if !status.success() {
                all_passed = false;
                println!("  ✗ FAILED: {criterion}");
                break;
            }
            println!("  ✓ passed");
        }

        if all_passed {
            println!("All acceptance criteria passed!");
        }
    } else if no_verify {
        // --no-verify means we skip criteria, so don't auto-verify
        all_passed = false;
    } else {
        // No acceptance criteria defined - treat as passed
        all_passed = true;
    }

    // Cleanup terminal resources based on backend type
    cleanup_terminal_for_stage(&stage_id, session_id.as_deref(), work_dir);

    // Cleanup session resources (update session status, remove signal)
    if let Some(ref sid) = session_id {
        cleanup_session_resources(&stage_id, sid, work_dir);
    }

    // Mark stage as completed
    stage.try_complete(None)?;
    save_stage(&stage, work_dir)?;

    if all_passed {
        // Attempt progressive merge into the merge point (base_branch)
        let repo_root = std::env::current_dir().context("Failed to get current directory")?;
        let merge_point = get_merge_point(work_dir)?;

        println!("Attempting progressive merge into '{merge_point}'...");
        match merge_completed_stage(&stage, &repo_root, &merge_point) {
            Ok(ProgressiveMergeResult::Success { files_changed }) => {
                println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
                stage.merged = true;
                save_stage(&stage, work_dir)?;
            }
            Ok(ProgressiveMergeResult::FastForward) => {
                println!("  ✓ Fast-forward merge into '{merge_point}'");
                stage.merged = true;
                save_stage(&stage, work_dir)?;
            }
            Ok(ProgressiveMergeResult::AlreadyMerged) => {
                println!("  ✓ Already up to date with '{merge_point}'");
                stage.merged = true;
                save_stage(&stage, work_dir)?;
            }
            Ok(ProgressiveMergeResult::NoBranch) => {
                println!("  → No branch to merge (already cleaned up)");
                stage.merged = true;
                save_stage(&stage, work_dir)?;
            }
            Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
                println!("  ✗ Merge conflict detected!");
                println!("    Conflicting files:");
                for file in &conflicting_files {
                    println!("      - {file}");
                }
                println!();
                println!("    Stage will remain active for conflict resolution.");
                println!("    Resolve conflicts and run: loom merge {stage_id}");
                stage.merge_conflict = true;
                save_stage(&stage, work_dir)?;
                // Don't trigger dependents when there's a conflict
                return Ok(());
            }
            Err(e) => {
                eprintln!("  ✗ Progressive merge failed: {e}");
                eprintln!("    Stage completed but merge skipped. Run manually:");
                eprintln!("    loom merge {stage_id}");
                // Continue with completion even if merge fails
            }
        }

        println!("Stage '{stage_id}' completed!");

        // Trigger dependent stages
        let triggered = trigger_dependents(&stage_id, work_dir)
            .context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
    } else {
        println!("Stage '{stage_id}' completed (skipped verification)");
    }

    Ok(())
}

/// Clean up terminal resources for a stage based on backend type
///
/// This function determines the backend type by checking the session's tmux_session field.
/// If tmux_session is present, it uses tmux backend cleanup. Otherwise, it's native backend
/// which doesn't require cleanup (processes are killed via PID by the orchestrator).
pub(super) fn cleanup_terminal_for_stage(
    stage_id: &str,
    session_id: Option<&str>,
    work_dir: &Path,
) {
    // Try to determine backend type from session data
    let backend_type = if let Some(sid) = session_id {
        detect_backend_from_session(sid, work_dir)
    } else {
        // No session ID - try tmux cleanup as best effort (backwards compatibility)
        BackendType::Tmux
    };

    match backend_type {
        BackendType::Tmux => {
            cleanup_tmux_for_stage(stage_id);
        }
        BackendType::Native => {
            // Native backend cleanup is handled by orchestrator via PID
            // No additional cleanup needed here
        }
    }
}

/// Kill tmux session for a stage (best-effort, doesn't require session_id)
fn cleanup_tmux_for_stage(stage_id: &str) {
    let tmux_name = format!("loom-{stage_id}");
    match Command::new("tmux")
        .args(["kill-session", "-t", &tmux_name])
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("Killed tmux session '{tmux_name}'");
        }
        Ok(_) => {
            // Session may not exist or already dead - this is fine
        }
        Err(e) => {
            eprintln!("Warning: failed to kill tmux session '{tmux_name}': {e}");
        }
    }
}
