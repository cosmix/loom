//! Wiring test verification - command-based integration testing

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::{CommandConfinement, WiringTest};
use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};

/// Default timeout for wiring test commands (30 seconds)
const WIRING_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify all wiring tests pass their success criteria
///
/// Runs each test command and validates the output against the defined success criteria.
/// Returns a VerificationGap for each failed validation.
///
/// `tree` is the stage worktree root the commands may use in full (`None`: the
/// working directory is the tree). `confinement` is the stage's resolved level
/// for plan-authored commands.
pub fn verify_wiring_tests(
    wiring_tests: &[WiringTest],
    working_dir: &Path,
    tree: Option<&Path>,
    confinement: CommandConfinement,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();
    for check in wiring_tests {
        let spec = CommandSpec::shell(check.command.as_str());
        let result = run_spec_in_optional_tree(
            &spec,
            tree,
            Some(working_dir),
            WIRING_TEST_TIMEOUT,
            confinement,
        )?;
        let assertions = AssertionSpec {
            expected_exit: check.success_criteria.exit_code.unwrap_or(0),
            stdout_contains: &check.success_criteria.stdout_contains,
            stdout_not_contains: &check.success_criteria.stdout_not_contains,
            stderr_contains: &check.success_criteria.stderr_contains,
            stderr_empty: check.success_criteria.stderr_empty == Some(true),
        };
        for failure in evaluate(&assertions, &result).failures {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!("Wiring test failed: {}: {failure}", check.name),
                format!("Check command: {}", check.command),
            ));
        }
    }
    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::SuccessCriteria;

    #[test]
    fn test_verify_wiring_tests_success() {
        let test = WiringTest {
            name: "echo test".to_string(),
            command: "echo hello".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                stdout_contains: vec!["hello".to_string()],
                stdout_not_contains: vec![],
                stderr_contains: vec![],
                stderr_empty: None,
            },
            description: Some("Test echo command".to_string()),
        };

        let working_dir = std::env::current_dir().unwrap();
        let gaps = verify_wiring_tests(&[test], &working_dir, None, CommandConfinement::default())
            .unwrap();

        assert!(
            gaps.is_empty(),
            "Expected no gaps for successful test, got: {:?}",
            gaps
        );
    }

    #[test]
    fn test_verify_wiring_tests_exit_code_failure() {
        let test = WiringTest {
            name: "false test".to_string(),
            command: "false".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                ..Default::default()
            },
            description: None,
        };

        let working_dir = std::env::current_dir().unwrap();
        let gaps = verify_wiring_tests(&[test], &working_dir, None, CommandConfinement::default())
            .unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].description.contains("exit code"));
        assert!(gaps[0].description.contains("false test"));
    }

    #[test]
    fn test_verify_wiring_tests_stdout_contains_failure() {
        let test = WiringTest {
            name: "missing pattern".to_string(),
            command: "echo hello".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                stdout_contains: vec!["goodbye".to_string()],
                ..Default::default()
            },
            description: None,
        };

        let working_dir = std::env::current_dir().unwrap();
        let gaps = verify_wiring_tests(&[test], &working_dir, None, CommandConfinement::default())
            .unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0]
            .description
            .contains("stdout missing expected pattern 'goodbye'"));
    }

    #[test]
    fn test_verify_wiring_tests_stdout_not_contains_failure() {
        let test = WiringTest {
            name: "forbidden pattern".to_string(),
            command: "echo error".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                stdout_not_contains: vec!["error".to_string()],
                ..Default::default()
            },
            description: None,
        };

        let working_dir = std::env::current_dir().unwrap();
        let gaps = verify_wiring_tests(&[test], &working_dir, None, CommandConfinement::default())
            .unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0]
            .description
            .contains("stdout contains forbidden pattern"));
    }

    #[test]
    fn test_verify_wiring_tests_stderr_empty_failure() {
        let test = WiringTest {
            name: "stderr check".to_string(),
            command: "sh -c 'echo error >&2'".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                stderr_empty: Some(true),
                ..Default::default()
            },
            description: None,
        };

        let working_dir = std::env::current_dir().unwrap();
        let gaps = verify_wiring_tests(&[test], &working_dir, None, CommandConfinement::default())
            .unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].description.contains("stderr was not empty"));
    }
}
