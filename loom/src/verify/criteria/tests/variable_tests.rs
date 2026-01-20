//! Tests for variable expansion in acceptance criteria

use std::fs;
use tempfile::tempdir;

use crate::models::stage::Stage;
use crate::verify::criteria::runner::run_acceptance;

#[test]
fn test_run_acceptance_with_worktree_variable() {
    // Create a temp directory to use as the working dir
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Use ${WORKTREE} variable in criterion - it should expand to working_dir
        stage.add_acceptance_criterion("test -d \"${WORKTREE}\"".to_string());
    } else {
        stage.add_acceptance_criterion("if exist \"${WORKTREE}\" (exit /b 0)".to_string());
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).unwrap();

    assert!(result.all_passed());
    // The stored command should be the original, not expanded
    assert!(result.results()[0].command.contains("${WORKTREE}"));
}

#[test]
fn test_run_acceptance_with_project_root_variable() {
    // Create a temp directory with a Cargo.toml to trigger PROJECT_ROOT detection
    let temp_dir = tempdir().expect("failed to create temp dir");
    fs::write(temp_dir.path().join("Cargo.toml"), "[package]").expect("failed to write Cargo.toml");

    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Use ${PROJECT_ROOT} variable - should be the dir with Cargo.toml
        stage.add_acceptance_criterion("test -f \"${PROJECT_ROOT}/Cargo.toml\"".to_string());
    } else {
        stage.add_acceptance_criterion(
            "if exist \"${PROJECT_ROOT}\\Cargo.toml\" (exit /b 0)".to_string(),
        );
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).unwrap();

    assert!(result.all_passed());
}

#[test]
fn test_run_acceptance_with_stage_id_variable() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Use ${STAGE_ID} variable - should expand to the stage's id
        stage.add_acceptance_criterion("test -n \"${STAGE_ID}\"".to_string());
    } else {
        stage.add_acceptance_criterion("echo %STAGE_ID%".to_string());
    }

    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
}

#[test]
fn test_run_acceptance_variables_in_setup() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Setup uses ${WORKTREE} variable
        stage.setup.push("cd ${WORKTREE}".to_string());
        stage.add_acceptance_criterion("pwd".to_string());
    } else {
        stage.setup.push("cd ${WORKTREE}".to_string());
        stage.add_acceptance_criterion("cd".to_string());
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).unwrap();

    assert!(result.all_passed());
}

#[test]
fn test_run_acceptance_unknown_variable_unchanged() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Unknown variable should remain unchanged, and the echo should succeed
        stage.add_acceptance_criterion("echo \"${UNKNOWN_VAR}\"".to_string());
    } else {
        stage.add_acceptance_criterion("echo ${UNKNOWN_VAR}".to_string());
    }

    let result = run_acceptance(&stage, None).unwrap();

    // Command should pass (echo always succeeds)
    assert!(result.all_passed());
}
