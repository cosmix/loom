//! Tests for acceptance criteria execution

use std::time::Duration;

use crate::verify::criteria::config::{CriteriaConfig, DEFAULT_COMMAND_TIMEOUT};
use crate::verify::criteria::executor::{run_single_criterion, run_single_criterion_with_timeout};
use crate::verify::criteria::result::{AcceptanceResult, CriterionResult};
use crate::verify::criteria::runner::run_acceptance;

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

#[test]
fn test_criteria_config_default() {
    let config = CriteriaConfig::default();
    assert_eq!(config.command_timeout, DEFAULT_COMMAND_TIMEOUT);
    assert_eq!(config.command_timeout, Duration::from_secs(300));
}

#[test]
fn test_criteria_config_with_timeout() {
    let config = CriteriaConfig::with_timeout(Duration::from_secs(60));
    assert_eq!(config.command_timeout, Duration::from_secs(60));
}

#[test]
fn test_run_acceptance_empty() {
    use crate::models::stage::Stage;

    let stage = Stage::new("test".to_string(), None);
    let result = run_acceptance(&stage, None).unwrap();

    assert!(result.all_passed());
    assert_eq!(result.results().len(), 0);
}

#[test]
fn test_run_acceptance_all_pass() {
    use crate::models::stage::Stage;

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
    use crate::models::stage::Stage;

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

#[test]
fn test_run_single_criterion_with_working_dir() {
    use std::path::PathBuf;

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

#[test]
fn test_run_acceptance_with_setup_commands() {
    use crate::models::stage::Stage;

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
    use crate::models::stage::Stage;

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
    use crate::models::stage::Stage;

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

#[test]
fn test_run_acceptance_with_worktree_variable() {
    use crate::models::stage::Stage;
    use tempfile::tempdir;

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
    use crate::models::stage::Stage;
    use std::fs;
    use tempfile::tempdir;

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
    use crate::models::stage::Stage;

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
    use crate::models::stage::Stage;
    use tempfile::tempdir;

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
    use crate::models::stage::Stage;

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
