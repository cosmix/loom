//! Check acceptance criteria without changing stage status
//!
//! Runs acceptance criteria and prints detailed results including full
//! stdout/stderr for each criterion. Increments fix_attempts on failure
//! but does NOT transition stage status.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::stage::{StageStatus, StageType};
use crate::verify::criteria::run_acceptance;
use crate::verify::transitions::{load_stage, save_stage};

use super::acceptance_runner::resolve_stage_execution_paths;

/// Default maximum fix attempts before suggesting dispute-criteria
const DEFAULT_MAX_FIX_ATTEMPTS: u32 = 3;

/// Run acceptance criteria for a stage and display detailed results.
///
/// This command:
/// 1. Loads the stage and validates its status (Executing or CompletedWithFailures)
/// 2. Runs all acceptance criteria
/// 3. Prints full stdout/stderr for each criterion
/// 4. Increments fix_attempts if any failed
/// 5. Does NOT change stage status
pub fn check_acceptance(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");
    let mut stage = load_stage(&stage_id, work_dir)?;

    // Validate stage is in a checkable state
    match stage.status {
        StageStatus::Executing | StageStatus::CompletedWithFailures => {}
        status => {
            bail!(
                "Stage '{stage_id}' is in {status} state. \
                 Only Executing or CompletedWithFailures stages can be checked."
            );
        }
    }

    if stage.acceptance.is_empty() {
        println!("No acceptance criteria defined for stage '{stage_id}'.");
        return Ok(());
    }

    // Resolve execution paths
    let execution_paths = resolve_stage_execution_paths(&stage)?;
    let acceptance_dir = execution_paths.acceptance_dir;

    if execution_paths.worktree_root.is_none() && stage.stage_type != StageType::Knowledge {
        bail!("Worktree not found for stage '{stage_id}'. Cannot run acceptance criteria.");
    }

    println!("Checking acceptance criteria for stage '{stage_id}'...");
    if let Some(ref dir) = acceptance_dir {
        println!("  (working directory: {})", dir.display());
    }
    println!();

    // Run acceptance criteria
    let result = run_acceptance(&stage, acceptance_dir.as_deref())
        .context("Failed to run acceptance criteria")?;

    // Print detailed results for each criterion
    let total = result.results().len();
    for (i, cr) in result.results().iter().enumerate() {
        let num = i + 1;
        println!("Criterion {num}: {}", cr.command);

        if cr.timed_out {
            println!("Result: TIMEOUT");
        } else if cr.success {
            println!("Result: PASSED");
        } else {
            println!(
                "Result: FAILED (exit code {})",
                cr.exit_code
                    .map_or("unknown".to_string(), |c| c.to_string())
            );
        }

        let duration_secs = cr.duration.as_secs_f64();
        println!("Duration: {duration_secs:.1}s");

        // Print stdout/stderr when non-empty (for all criteria, not just failures)
        if !cr.stdout.is_empty() {
            println!("stdout:");
            for line in cr.stdout.lines() {
                println!("  {line}");
            }
        }
        if !cr.stderr.is_empty() {
            println!("stderr:");
            for line in cr.stderr.lines() {
                println!("  {line}");
            }
        }

        if num < total {
            println!();
        }
    }

    println!();
    let passed = result.passed_count();
    println!("Summary: {passed}/{total} passed");

    if !result.all_passed() {
        stage.fix_attempts += 1;
        save_stage(&stage, work_dir)?;

        let max = stage.max_retries.unwrap_or(DEFAULT_MAX_FIX_ATTEMPTS);
        println!("Fix attempts: {}/{max}", stage.fix_attempts);

        if stage.fix_attempts >= max {
            println!();
            println!("Warning: Fix attempt limit reached ({max}/{max}).");
            println!(
                "  If the acceptance criteria are incorrect, use \
                 'loom stage dispute-criteria {stage_id}' to challenge them."
            );
        } else {
            println!(
                "Hint: Fix the issues and run 'loom stage check-acceptance {stage_id}' again."
            );
        }

        let failed = result.failed_count();
        bail!("Acceptance check failed for stage '{stage_id}': {failed}/{total} criteria failed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use crate::verify::transitions::save_stage;
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
    fn test_check_acceptance_rejects_invalid_status() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::Completed);
        save_stage(&stage, &work_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = check_acceptance("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Completed"));
        assert!(err.contains("Executing or CompletedWithFailures"));
    }

    #[test]
    #[serial]
    fn test_check_acceptance_accepts_executing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::Executing);
        save_stage(&stage, &work_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = check_acceptance("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        // Fails because worktree doesn't exist, but status check passes
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Worktree not found"));
    }

    #[test]
    #[serial]
    fn test_check_acceptance_accepts_completed_with_failures() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let stage = create_test_stage("test-stage", StageStatus::CompletedWithFailures);
        save_stage(&stage, &work_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = check_acceptance("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Worktree not found"));
    }

    #[test]
    #[serial]
    fn test_check_acceptance_increments_fix_attempts_on_failure() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        // Create a worktree directory so path resolution works
        let worktree_dir = temp_dir.path().join(".worktrees/test-stage");
        std::fs::create_dir_all(&worktree_dir).unwrap();

        let mut stage = create_test_stage("test-stage", StageStatus::Executing);
        stage.acceptance = vec!["false".to_string()]; // always fails
        stage.working_dir = Some(".".to_string());
        stage.fix_attempts = 0;
        save_stage(&stage, &work_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = check_acceptance("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_err());

        // Reload stage and check fix_attempts was incremented
        let reloaded = load_stage("test-stage", &work_dir).unwrap();
        assert_eq!(reloaded.fix_attempts, 1);
    }

    #[test]
    #[serial]
    fn test_check_acceptance_no_criteria() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();

        let mut stage = create_test_stage("test-stage", StageStatus::Executing);
        stage.acceptance = vec![];
        save_stage(&stage, &work_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = check_acceptance("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        // Empty acceptance check exits early before worktree resolution
        assert!(result.is_ok());
    }
}
