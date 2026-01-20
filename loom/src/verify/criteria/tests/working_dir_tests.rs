//! Tests for working directory functionality

use std::path::PathBuf;

use crate::verify::criteria::executor::run_single_criterion;

#[test]
fn test_run_single_criterion_with_working_dir() {
    // Create temp dir and verify command runs in it
    let temp_dir = std::env::temp_dir();
    let command = if cfg!(target_family = "unix") {
        "pwd"
    } else {
        "cd"
    };

    let result = run_single_criterion(command, Some(&temp_dir)).unwrap();

    assert!(result.success);
    assert_eq!(result.exit_code, Some(0));
    // The output should contain the temp directory path
    let canonical_temp = temp_dir.canonicalize().unwrap_or(temp_dir.clone());
    let stdout_path = PathBuf::from(result.stdout.trim());
    let canonical_stdout = stdout_path.canonicalize().unwrap_or(stdout_path);
    assert_eq!(canonical_stdout, canonical_temp);
}
