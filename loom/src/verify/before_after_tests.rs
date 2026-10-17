use super::*;
use std::env;

#[test]
fn test_before_after_stage_checks_pass() {
    // Simulate a "before" check: command exits 1 (feature doesn't exist yet)
    let before_checks = vec![TruthCheck {
        command: "exit 1".to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: Some(1),
        description: Some("Feature test should fail before implementation".to_string()),
    }];

    let working_dir = env::temp_dir();
    let gaps = run_before_stage_checks(&before_checks, &working_dir, None).unwrap();
    assert!(
        gaps.is_empty(),
        "Before-stage checks should pass when pre-conditions match"
    );
}

#[test]
fn test_before_stage_checks_fail_when_precondition_not_met() {
    // Before check expects exit 1, but command exits 0 (feature already exists)
    let before_checks = vec![TruthCheck {
        command: "exit 0".to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: Some(1),
        description: Some("Feature should not exist yet".to_string()),
    }];

    let working_dir = env::temp_dir();
    let gaps = run_before_stage_checks(&before_checks, &working_dir, None).unwrap();
    assert_eq!(
        gaps.len(),
        1,
        "Should report gap when pre-condition not met"
    );
}

#[test]
fn test_after_stage_checks_pass() {
    // Simulate an "after" check: command exits 0 (feature works)
    let after_checks = vec![TruthCheck {
        command: "echo 'feature works'".to_string(),
        stdout_contains: vec!["feature works".to_string()],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: Some(0),
        description: Some("Feature should work after implementation".to_string()),
    }];

    let working_dir = env::temp_dir();
    let gaps = run_after_stage_checks(
        &after_checks,
        &working_dir,
        None,
        CommandConfinement::default(),
    )
    .unwrap();
    assert!(
        gaps.is_empty(),
        "After-stage checks should pass when post-conditions match"
    );
}

#[test]
fn test_after_stage_checks_fail_when_postcondition_not_met() {
    // After check expects stdout to contain "feature works", but it doesn't
    let after_checks = vec![TruthCheck {
        command: "echo 'something else'".to_string(),
        stdout_contains: vec!["feature works".to_string()],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: None,
        description: Some("Feature output check".to_string()),
    }];

    let working_dir = env::temp_dir();
    let gaps = run_after_stage_checks(
        &after_checks,
        &working_dir,
        None,
        CommandConfinement::default(),
    )
    .unwrap();
    assert_eq!(
        gaps.len(),
        1,
        "Should report gap when post-condition not met"
    );
}

#[test]
fn test_before_after_empty_checks() {
    let working_dir = env::temp_dir();

    let gaps = run_before_stage_checks(&[], &working_dir, None).unwrap();
    assert!(
        gaps.is_empty(),
        "Empty before checks should produce no gaps"
    );

    let gaps =
        run_after_stage_checks(&[], &working_dir, None, CommandConfinement::default()).unwrap();
    assert!(gaps.is_empty(), "Empty after checks should produce no gaps");
}

#[test]
fn test_before_stage_stdout_not_contains() {
    // Before check: stdout should NOT contain "FeatureX" (feature doesn't exist)
    let checks = vec![TruthCheck {
        command: "echo 'no features here'".to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec!["FeatureX".to_string()],
        stderr_empty: None,
        exit_code: None,
        description: Some("FeatureX should not appear before implementation".to_string()),
    }];

    let working_dir = env::temp_dir();
    let gaps = run_before_stage_checks(&checks, &working_dir, None).unwrap();
    assert!(
        gaps.is_empty(),
        "Check should pass when forbidden pattern is absent"
    );
}

#[test]
fn test_after_stage_multiple_checks() {
    let checks = vec![
        TruthCheck {
            command: "echo 'test passed'".to_string(),
            stdout_contains: vec!["test passed".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(0),
            description: Some("First post-condition".to_string()),
        },
        TruthCheck {
            command: "echo 'integration ok'".to_string(),
            stdout_contains: vec!["integration ok".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(0),
            description: Some("Second post-condition".to_string()),
        },
    ];

    let working_dir = env::temp_dir();
    let gaps =
        run_after_stage_checks(&checks, &working_dir, None, CommandConfinement::default()).unwrap();
    assert!(gaps.is_empty(), "All after-stage checks should pass");
}
