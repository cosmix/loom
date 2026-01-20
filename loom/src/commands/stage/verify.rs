//! Stage verify command - re-run acceptance criteria and complete a stage

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::git::get_branch_head;
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::{get_merge_point, merge_completed_stage, ProgressiveMergeResult};
use crate::verify::criteria::run_acceptance;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::acceptance_runner::resolve_acceptance_dir;
use super::criteria_runner::reload_acceptance_from_plan;

/// Re-run acceptance criteria and complete a stage that previously failed.
///
/// This command is useful when:
/// - A stage is in CompletedWithFailures state and you've fixed the issues
/// - A stage is in Executing state and you want to manually verify/complete it
///
/// The command will:
/// 1. Validate stage is in CompletedWithFailures or Executing state
/// 2. Reload acceptance criteria from plan file (unless --no-reload)
/// 3. Run acceptance criteria
/// 4. If pass: complete stage with merge
/// 5. If fail: save updated criteria and exit with message
pub fn verify(stage_id: String, no_reload: bool) -> Result<()> {
    let work_dir = Path::new(".work");
    let mut stage = load_stage(&stage_id, work_dir)?;

    // Validate stage is in a verifiable state
    match stage.status {
        StageStatus::CompletedWithFailures | StageStatus::Executing => {}
        status => {
            bail!(
                "Stage '{stage_id}' is in {status} state. Only CompletedWithFailures or Executing stages can be verified."
            );
        }
    }

    // Reload acceptance criteria from plan file unless --no-reload
    if !no_reload {
        reload_acceptance_from_plan(&mut stage, work_dir)?;
    }

    // Resolve worktree path from stage
    let worktree_path: Option<PathBuf> = stage
        .worktree
        .as_ref()
        .map(|w| PathBuf::from(".worktrees").join(w))
        .filter(|p| p.exists());

    if worktree_path.is_none() && stage.stage_type != StageType::Knowledge {
        bail!("Worktree not found for stage '{stage_id}'. Cannot run acceptance criteria.");
    }

    // Resolve acceptance criteria working directory
    let acceptance_dir: Option<PathBuf> =
        resolve_acceptance_dir(worktree_path.as_deref(), stage.working_dir.as_deref());

    // Run acceptance criteria
    let acceptance_result = if !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        if let Some(ref dir) = acceptance_dir {
            println!("  (working directory: {})", dir.display());
        }

        let result = run_acceptance(&stage, acceptance_dir.as_deref())
            .context("Failed to run acceptance criteria")?;

        for criterion_result in result.results() {
            if criterion_result.success {
                println!("  ✓ passed: {}", criterion_result.command);
            } else if criterion_result.timed_out {
                println!("  ✗ TIMEOUT: {}", criterion_result.command);
            } else {
                println!("  ✗ FAILED: {}", criterion_result.command);
            }
        }

        if result.all_passed() {
            println!("All acceptance criteria passed!");
        }
        result.all_passed()
    } else {
        println!("No acceptance criteria defined, treating as passed.");
        true
    };

    // Handle acceptance failure
    if !acceptance_result {
        // Keep stage in current state (CompletedWithFailures or update to it)
        if stage.status == StageStatus::Executing {
            stage.try_complete_with_failures()?;
        }
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' verification failed - acceptance criteria did not pass");
        println!("  Fix the issues and run 'loom stage verify {stage_id}' again");
        return Ok(());
    }

    // Handle knowledge stages (no merge required)
    if stage.stage_type == StageType::Knowledge {
        stage.merged = true;
        stage.try_complete(None)?;
        save_stage(&stage, work_dir)?;

        println!("Knowledge stage '{stage_id}' verified and completed!");
        println!("  (merged=true auto-set, no git merge required for knowledge stages)");

        let triggered = trigger_dependents(&stage_id, work_dir)
            .context("Failed to trigger dependent stages")?;
        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
        return Ok(());
    }

    // For standard stages: attempt progressive merge
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    let merge_point = get_merge_point(work_dir)?;

    // Capture the completed commit SHA before merge
    let branch_name = format!("loom/{stage_id}");
    let completed_commit = get_branch_head(&branch_name, &repo_root).ok();

    println!("Attempting progressive merge into '{merge_point}'...");
    match merge_completed_stage(&stage, &repo_root, &merge_point) {
        Ok(ProgressiveMergeResult::Success { files_changed }) => {
            println!("  ✓ Merged {files_changed} file(s) into '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
        }
        Ok(ProgressiveMergeResult::FastForward) => {
            println!("  ✓ Fast-forward merge into '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
        }
        Ok(ProgressiveMergeResult::AlreadyMerged) => {
            println!("  ✓ Already up to date with '{merge_point}'");
            stage.completed_commit = completed_commit;
            stage.merged = true;
        }
        Ok(ProgressiveMergeResult::NoBranch) => {
            println!("  → No branch to merge (already cleaned up)");
            stage.merged = true;
        }
        Ok(ProgressiveMergeResult::Conflict { conflicting_files }) => {
            println!("  ✗ Merge conflict detected!");
            println!("    Conflicting files:");
            for file in &conflicting_files {
                println!("      - {file}");
            }
            println!();
            println!("    Stage transitioning to MergeConflict status.");
            println!("    Resolve conflicts and run: loom stage merge-complete {stage_id}");
            stage.try_mark_merge_conflict()?;
            save_stage(&stage, work_dir)?;
            return Ok(());
        }
        Err(e) => {
            eprintln!("Progressive merge failed: {e}");
            stage.try_mark_merge_blocked()?;
            save_stage(&stage, work_dir)?;
            eprintln!("Stage '{stage_id}' marked as MergeBlocked");
            eprintln!("  Fix the issue and run: loom stage verify {stage_id}");
            return Ok(());
        }
    }

    // Mark stage as completed
    stage.try_complete(None)?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' verified and completed!");

    // Trigger dependent stages
    let triggered =
        trigger_dependents(&stage_id, work_dir).context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use chrono::Utc;
    use serial_test::serial;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: "Test Stage".to_string(),
            description: None,
            status,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec!["echo test".to_string()],
            setup: vec![],
            files: vec![],
            stage_type: StageType::Standard,
            plan_id: None,
            worktree: Some(id.to_string()),
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
            auto_merge: None,
            working_dir: Some(".".to_string()),
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    #[serial]
    fn test_verify_rejects_invalid_status() {
        // Test that verify rejects stages not in CompletedWithFailures or Executing
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        // Create a stage in Completed status
        let stage = create_test_stage("test-stage", StageStatus::Completed);
        save_stage(&stage, &work_dir).unwrap();

        // Save and restore current directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = verify("test-stage".to_string(), true);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Completed"));
        assert!(err.contains("CompletedWithFailures or Executing"));
    }

    #[test]
    #[serial]
    fn test_verify_accepts_completed_with_failures() {
        // This test verifies the status check passes for CompletedWithFailures
        // Full integration testing requires worktree setup
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::CompletedWithFailures);
        save_stage(&stage, &work_dir).unwrap();

        // Save and restore current directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // This will fail because worktree doesn't exist, but the status check passes
        let result = verify("test-stage".to_string(), true);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Worktree not found"));
    }

    #[test]
    #[serial]
    fn test_verify_accepts_executing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::Executing);
        save_stage(&stage, &work_dir).unwrap();

        // Save and restore current directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = verify("test-stage".to_string(), true);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Worktree not found"));
    }
}
