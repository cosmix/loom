//! Tests for CriterionResult and AcceptanceResult

use std::time::Duration;

use crate::verify::criteria::result::{AcceptanceResult, CriterionResult};

#[test]
fn test_criterion_result_new() {
    let result = CriterionResult::new(
        "echo test".to_string(),
        true,
        "test\n".to_string(),
        String::new(),
        Some(0),
        Duration::from_millis(100),
        false,
    );

    assert!(result.passed());
    assert_eq!(result.command, "echo test");
    assert_eq!(result.stdout, "test\n");
    assert_eq!(result.stderr, "");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.duration, Duration::from_millis(100));
    assert!(!result.timed_out);
}

#[test]
fn test_criterion_result_summary() {
    let result = CriterionResult::new(
        "cargo test".to_string(),
        false,
        String::new(),
        "error".to_string(),
        Some(1),
        Duration::from_millis(500),
        false,
    );

    let summary = result.summary();
    assert!(summary.contains("FAILED"));
    assert!(summary.contains("cargo test"));
    assert!(summary.contains("500ms"));
    assert!(summary.contains("exit code: Some(1)"));
}

#[test]
fn test_criterion_result_summary_timeout() {
    let result = CriterionResult::new(
        "sleep 1000".to_string(),
        false,
        String::new(),
        "[Process killed after 5s timeout]".to_string(),
        None,
        Duration::from_secs(5),
        true,
    );

    let summary = result.summary();
    assert!(summary.contains("TIMEOUT"));
    assert!(summary.contains("sleep 1000"));
}

#[test]
fn test_acceptance_result_all_passed() {
    let results = vec![
        CriterionResult::new(
            "test1".to_string(),
            true,
            "ok".to_string(),
            String::new(),
            Some(0),
            Duration::from_millis(100),
            false,
        ),
        CriterionResult::new(
            "test2".to_string(),
            true,
            "ok".to_string(),
            String::new(),
            Some(0),
            Duration::from_millis(200),
            false,
        ),
    ];

    let acceptance = AcceptanceResult::AllPassed {
        results: results.clone(),
    };

    assert!(acceptance.all_passed());
    assert_eq!(acceptance.passed_count(), 2);
    assert_eq!(acceptance.failed_count(), 0);
    assert_eq!(acceptance.failures().len(), 0);
    assert_eq!(acceptance.total_duration(), Duration::from_millis(300));
}

#[test]
fn test_acceptance_result_failed() {
    let results = vec![
        CriterionResult::new(
            "test1".to_string(),
            true,
            "ok".to_string(),
            String::new(),
            Some(0),
            Duration::from_millis(100),
            false,
        ),
        CriterionResult::new(
            "test2".to_string(),
            false,
            String::new(),
            "error".to_string(),
            Some(1),
            Duration::from_millis(200),
            false,
        ),
    ];

    let failures = vec!["test2 failed".to_string()];
    let acceptance = AcceptanceResult::Failed {
        results: results.clone(),
        failures,
    };

    assert!(!acceptance.all_passed());
    assert_eq!(acceptance.passed_count(), 1);
    assert_eq!(acceptance.failed_count(), 1);
    assert_eq!(acceptance.failures().len(), 1);
    assert_eq!(acceptance.total_duration(), Duration::from_millis(300));
}
