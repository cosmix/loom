//! Tests for acceptance runner

use crate::models::stage::Stage;
use crate::verify::criteria::runner::run_acceptance;

#[test]
fn test_run_acceptance_empty() {
    let stage = Stage::new("test".to_string(), None);
    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.results().len(), 0);
}

#[test]
fn test_run_acceptance_all_pass() {
    let mut stage = Stage::new("test".to_string(), None);
    let command = if cfg!(target_family = "unix") {
        "true"
    } else {
        "exit /b 0"
    };
    stage.add_acceptance_criterion(command.to_string());
    stage.add_acceptance_criterion(command.to_string());

    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.results().len(), 2);
    assert_eq!(result.passed_count(), 2);
    assert_eq!(result.failed_count(), 0);
}

#[test]
fn test_run_acceptance_some_fail() {
    let mut stage = Stage::new("test".to_string(), None);

    if cfg!(target_family = "unix") {
        stage.add_acceptance_criterion("true".to_string());
        stage.add_acceptance_criterion("false".to_string());
    } else {
        stage.add_acceptance_criterion("exit /b 0".to_string());
        stage.add_acceptance_criterion("exit /b 1".to_string());
    }

    let result = run_acceptance(&stage, None).unwrap();

    assert!(!result.all_passed());
    assert_eq!(result.results().len(), 2);
    assert_eq!(result.passed_count(), 1);
    assert_eq!(result.failed_count(), 1);
    assert_eq!(result.failures().len(), 1);
}
