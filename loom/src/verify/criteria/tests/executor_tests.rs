//! Tests for executor module

use std::time::Duration;

use crate::verify::criteria::executor::{run_single_criterion, run_single_criterion_with_timeout};

#[test]
fn test_run_single_criterion_success() {
    let command = if cfg!(target_family = "unix") {
        "echo 'hello world'"
    } else {
        "echo hello world"
    };

    let result = run_single_criterion(command, None).unwrap();

    assert!(result.success);
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.contains("hello world"));
    assert!(result.duration > Duration::from_nanos(0));
    assert!(!result.timed_out);
}

#[test]
fn test_run_single_criterion_failure() {
    let command = if cfg!(target_family = "unix") {
        "exit 42"
    } else {
        "exit /b 42"
    };

    let result = run_single_criterion(command, None).unwrap();

    assert!(!result.success);
    assert_eq!(result.exit_code, Some(42));
    assert!(!result.timed_out);
}

#[test]
fn test_run_single_criterion_timeout() {
    // Only run on Unix - sleep command behavior differs on Windows
    if cfg!(target_family = "unix") {
        // Use a very short timeout (100ms) with a command that sleeps for 10 seconds
        let result =
            run_single_criterion_with_timeout("sleep 10", None, Duration::from_millis(100))
                .unwrap();

        assert!(!result.success);
        assert!(result.timed_out);
        assert!(result.exit_code.is_none()); // killed process has no exit code
        assert!(result.stderr.contains("timeout"));
        // Duration should be close to the timeout, not 10 seconds
        assert!(result.duration < Duration::from_secs(1));
    }
}
