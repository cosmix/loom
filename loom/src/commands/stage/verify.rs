//! Stage verify command - re-run acceptance criteria and complete a stage

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

use super::acceptance_runner::{
    resolve_stage_execution_paths, run_acceptance_with_display, AcceptanceDisplayOptions,
};
use super::criteria_runner::reload_acceptance_from_plan;
use super::progressive_complete::attempt_progressive_merge;

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

    // Resolve worktree and acceptance execution paths via shared logic
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let worktree_path: Option<PathBuf> = execution_paths.worktree_root;
    let acceptance_dir: Option<PathBuf> = execution_paths.acceptance_dir;

    if worktree_path.is_none() && stage.stage_type != StageType::Knowledge {
        bail!("Worktree not found for stage '{stage_id}'. Cannot run acceptance criteria.");
    }

    // Run acceptance criteria
    let acceptance_result = run_acceptance_with_display(
        &stage,
        &stage_id,
        acceptance_dir.as_deref(),
        AcceptanceDisplayOptions {
            stage_label: Some("stage"),
            show_empty_message: true,
        },
    )?;

    // Handle acceptance failure
    if !acceptance_result {
        // If stage is Executing, keep it Executing (don't transition to CompletedWithFailures)
        // If stage is already CompletedWithFailures, save updated criteria only
        if stage.status == StageStatus::CompletedWithFailures {
            // Save any updated acceptance criteria (from plan reload) without state change
            save_stage(&stage, work_dir)?;
        }
        // If Executing, don't save or transition - just bail
        eprintln!("Verification FAILED for stage '{stage_id}' - acceptance criteria did not pass");
        eprintln!("  Fix the issues and run 'loom stage verify {stage_id}' again");
        bail!("Verification failed for stage '{stage_id}'");
    }

    // Run goal-backward verification if defined
    {
        let config = crate::fs::work_dir::load_config_required(work_dir)?;
        let plan_path = config
            .source_path()
            .context("No plan source path configured in .work/config.toml")?;
        let plan = crate::plan::parser::parse_plan(&plan_path)
            .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;

        if let Some(stage_def) = plan.stages.iter().find(|s| s.id == stage_id) {
            if stage_def.has_any_goal_checks() {
                println!("Running goal-backward verification...");
                let verify_dir = acceptance_dir.as_deref().unwrap_or(Path::new("."));
                let goal_result = crate::verify::goal_backward::run_goal_backward_verification(
                    stage_def, verify_dir,
                )?;

                if !goal_result.is_passed() {
                    for gap in goal_result.gaps() {
                        eprintln!("  ✗ {:?}: {}", gap.gap_type, gap.description);
                        eprintln!("    → {}", gap.suggestion);
                    }
                    eprintln!();
                    eprintln!("Goal-backward verification FAILED for stage '{stage_id}'");
                    eprintln!("  Fix the issues and run 'loom stage verify {stage_id}' again");
                    bail!("Goal-backward verification failed for stage '{stage_id}'");
                }
                println!("Goal-backward verification passed!");
            }
        }
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

    // Use the shared progressive merge logic
    use super::progressive_complete::MergeOutcome;
    match attempt_progressive_merge(&mut stage, &repo_root, work_dir)? {
        MergeOutcome::Success => {
            // Mark stage as completed
            stage.try_complete(None)?;
            save_stage(&stage, work_dir)?;

            println!("Stage '{stage_id}' verified and completed!");

            // Trigger dependent stages
            let triggered = trigger_dependents(&stage_id, work_dir)
                .context("Failed to trigger dependent stages")?;

            if !triggered.is_empty() {
                println!("Triggered {} dependent stage(s):", triggered.len());
                for dep_id in &triggered {
                    println!("  → {dep_id}");
                }
            }

            Ok(())
        }
        MergeOutcome::Conflict | MergeOutcome::Blocked => {
            // Stage already saved in conflict/blocked state by attempt_progressive_merge
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use serial_test::serial;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: "Test Stage".to_string(),
            status,
            acceptance: vec!["echo test".to_string()],
            worktree: Some(id.to_string()),
            ..Stage::default()
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
