//! Truth verification - observable behaviors that must work

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::{CommandConfinement, TruthCheck};
use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};

/// Default timeout for truth commands (30 seconds)
const TRUTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify enhanced truth checks with extended success criteria
///
/// `confinement` is the stage's resolved level for plan-authored commands.
pub fn verify_truth_checks(
    truth_checks: &[TruthCheck],
    working_dir: &Path,
    confinement: CommandConfinement,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();
    for check in truth_checks {
        let spec = CommandSpec::shell(check.command.as_str());
        let result = run_spec_with_timeout(&spec, Some(working_dir), TRUTH_TIMEOUT, confinement)?;
        let assertions = AssertionSpec {
            expected_exit: check.exit_code.unwrap_or(0),
            stdout_contains: &check.stdout_contains,
            stdout_not_contains: &check.stdout_not_contains,
            stderr_empty: check.stderr_empty == Some(true),
            ..Default::default()
        };
        for failure in evaluate(&assertions, &result).failures {
            gaps.push(VerificationGap::new(
                GapType::TruthFailed,
                format!(
                    "Truth check failed: {}: {failure}",
                    check.description.as_deref().unwrap_or(&check.command)
                ),
                format!("Check command: {}", check.command),
            ));
        }
    }
    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_verify_truth_checks_exit_code_success() {
        let checks = vec![TruthCheck {
            command: "echo 'test' && exit 0".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(0),
            description: Some("Test exit code 0".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(result.is_empty(), "Expected no gaps for successful check");
    }

    #[test]
    fn test_verify_truth_checks_exit_code_failure() {
        let checks = vec![TruthCheck {
            command: "exit 1".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(0),
            description: Some("Test exit code failure".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert_eq!(result.len(), 1, "Expected one gap for failed exit code");
        assert!(result[0].description.contains("Truth check failed"));
    }

    #[test]
    fn test_verify_truth_checks_stdout_contains_success() {
        let checks = vec![TruthCheck {
            command: "echo 'Hello World'".to_string(),
            stdout_contains: vec!["Hello".to_string(), "World".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: None,
            description: Some("Test stdout contains".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(
            result.is_empty(),
            "Expected no gaps when stdout contains patterns"
        );
    }

    #[test]
    fn test_verify_truth_checks_stdout_contains_failure() {
        let checks = vec![TruthCheck {
            command: "echo 'Hello'".to_string(),
            stdout_contains: vec!["World".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: None,
            description: Some("Test stdout missing pattern".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert_eq!(result.len(), 1, "Expected one gap for missing pattern");
        assert!(result[0].description.contains("missing expected pattern"));
    }

    #[test]
    fn test_verify_truth_checks_stdout_not_contains_success() {
        let checks = vec![TruthCheck {
            command: "echo 'Hello World'".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec!["ERROR".to_string()],
            stderr_empty: None,
            exit_code: None,
            description: Some("Test stdout not contains".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(
            result.is_empty(),
            "Expected no gaps when stdout doesn't contain forbidden patterns"
        );
    }

    #[test]
    fn test_verify_truth_checks_stdout_not_contains_failure() {
        let checks = vec![TruthCheck {
            command: "echo 'ERROR: something went wrong'".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec!["ERROR".to_string()],
            stderr_empty: None,
            exit_code: None,
            description: Some("Test stdout forbidden pattern".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert_eq!(result.len(), 1, "Expected one gap for forbidden pattern");
        assert!(result[0].description.contains("forbidden pattern"));
    }

    #[test]
    fn test_verify_truth_checks_stderr_empty_success() {
        let checks = vec![TruthCheck {
            command: "echo 'test'".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: Some(true),
            exit_code: None,
            description: Some("Test stderr empty".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(result.is_empty(), "Expected no gaps when stderr is empty");
    }

    #[test]
    fn test_verify_truth_checks_stderr_empty_failure() {
        let checks = vec![TruthCheck {
            command: "echo 'error' >&2".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: Some(true),
            exit_code: None,
            description: Some("Test stderr not empty".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert_eq!(result.len(), 1, "Expected one gap for non-empty stderr");
        assert!(result[0].description.contains("stderr was not empty"));
    }

    #[test]
    fn test_verify_truth_checks_multiple_criteria() {
        let checks = vec![TruthCheck {
            command: "echo 'Success: operation completed'".to_string(),
            stdout_contains: vec!["Success".to_string()],
            stdout_not_contains: vec!["ERROR".to_string(), "FAIL".to_string()],
            stderr_empty: Some(true),
            exit_code: Some(0),
            description: Some("Test all criteria".to_string()),
        }];

        let working_dir = env::temp_dir();
        let result =
            verify_truth_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(result.is_empty(), "Expected no gaps when all criteria pass");
    }

    // Tests for truncate moved to verify/utils.rs
}
