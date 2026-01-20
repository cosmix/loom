//! Tests for setup command functionality

use crate::models::stage::Stage;
use crate::verify::criteria::runner::run_acceptance;

#[test]
fn test_run_acceptance_with_setup_commands() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Setup creates an environment variable, criterion checks it exists
        stage.setup.push("export TEST_VAR=hello".to_string());
        stage.add_acceptance_criterion("test -n \"$TEST_VAR\"".to_string());
    } else {
        // Windows: set var and check it
        stage.setup.push("set TEST_VAR=hello".to_string());
        stage.add_acceptance_criterion(
            "if defined TEST_VAR (exit /b 0) else (exit /b 1)".to_string(),
        );
    }

    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.passed_count(), 1);
    // Verify the result stores original command, not the combined one
    assert!(!result.results()[0].command.contains("export"));
}

#[test]
fn test_run_acceptance_setup_failure_fails_criterion() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Setup command that fails
        stage.setup.push("false".to_string());
        stage.add_acceptance_criterion("true".to_string());
    } else {
        stage.setup.push("exit /b 1".to_string());
        stage.add_acceptance_criterion("exit /b 0".to_string());
    }

    let result = run_acceptance(&stage, None).unwrap();

    // Even though the criterion itself would pass, setup failure causes failure
    assert!(!result.all_passed());
    assert_eq!(result.failed_count(), 1);
}

#[test]
fn test_run_acceptance_multiple_setup_commands() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        // Multiple setup commands chained
        stage.setup.push("export A=1".to_string());
        stage.setup.push("export B=2".to_string());
        stage.add_acceptance_criterion("test -n \"$A\" && test -n \"$B\"".to_string());
    } else {
        stage.setup.push("set A=1".to_string());
        stage.setup.push("set B=2".to_string());
        stage.add_acceptance_criterion(
            "if defined A if defined B (exit /b 0) else (exit /b 1)".to_string(),
        );
    }

    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
}
