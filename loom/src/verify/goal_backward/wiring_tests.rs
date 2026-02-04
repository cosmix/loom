//! Wiring test verification - command-based integration testing

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::WiringTest;
use crate::verify::criteria::run_single_criterion_with_timeout;

/// Default timeout for wiring test commands (30 seconds)
const WIRING_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify all wiring tests pass their success criteria
///
/// Runs each test command and validates the output against the defined success criteria.
/// Returns a VerificationGap for each failed validation.
pub fn verify_wiring_tests(
    wiring_tests: &[WiringTest],
    working_dir: &Path,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for test in wiring_tests {
        // Run the test command
        let result = run_single_criterion_with_timeout(
            &test.command,
            Some(working_dir),
            WIRING_TEST_TIMEOUT,
        )?;

        // Check if timed out
        if result.timed_out {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!(
                    "Wiring test '{}' timed out after {}s",
                    test.name,
                    WIRING_TEST_TIMEOUT.as_secs()
                ),
                format!("Check command: {}", test.command),
            ));
            continue;
        }

        // Validate exit code
        let expected_exit_code = test.success_criteria.exit_code.unwrap_or(0);
        let actual_exit_code = result.exit_code.unwrap_or(-1);
        if actual_exit_code != expected_exit_code {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!(
                    "Wiring test '{}' failed: exit code {} (expected {})",
                    test.name, actual_exit_code, expected_exit_code
                ),
                format!("Check command: {}", test.command),
            ));
            continue;
        }

        // Validate stdout_contains
        for pattern in &test.success_criteria.stdout_contains {
            if !result.stdout.contains(pattern) {
                let preview = truncate_output(&result.stdout, 200);
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!(
                        "Wiring test '{}' failed: stdout missing '{}'",
                        test.name, pattern
                    ),
                    format!(
                        "Expected stdout to contain: '{}'. Got: {}",
                        pattern, preview
                    ),
                ));
            }
        }

        // Validate stdout_not_contains
        for pattern in &test.success_criteria.stdout_not_contains {
            if result.stdout.contains(pattern) {
                let preview = truncate_output(&result.stdout, 200);
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!(
                        "Wiring test '{}' failed: stdout contains forbidden pattern '{}'",
                        test.name, pattern
                    ),
                    format!(
                        "Expected stdout to NOT contain: '{}'. Got: {}",
                        pattern, preview
                    ),
                ));
            }
        }

        // Validate stderr_contains
        for pattern in &test.success_criteria.stderr_contains {
            if !result.stderr.contains(pattern) {
                let preview = truncate_output(&result.stderr, 200);
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!(
                        "Wiring test '{}' failed: stderr missing '{}'",
                        test.name, pattern
                    ),
                    format!(
                        "Expected stderr to contain: '{}'. Got: {}",
                        pattern, preview
                    ),
                ));
            }
        }

        // Validate stderr_empty
        if let Some(true) = test.success_criteria.stderr_empty {
            if !result.stderr.is_empty() {
                let preview = truncate_output(&result.stderr, 200);
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!("Wiring test '{}' failed: stderr not empty", test.name),
                    format!("Expected empty stderr. Got: {}", preview),
                ));
            }
        }
    }

    Ok(gaps)
}

/// Truncate output to a specified length for display in error messages
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() <= max_len {
        output.to_string()
    } else {
        format!("{}...", &output[..max_len])
    }
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
        let gaps = verify_wiring_tests(&[test], &working_dir).unwrap();

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
        let gaps = verify_wiring_tests(&[test], &working_dir).unwrap();

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
        let gaps = verify_wiring_tests(&[test], &working_dir).unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].description.contains("stdout missing 'goodbye'"));
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
        let gaps = verify_wiring_tests(&[test], &working_dir).unwrap();

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
        let gaps = verify_wiring_tests(&[test], &working_dir).unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].description.contains("stderr not empty"));
    }

    #[test]
    fn test_truncate_output() {
        let short = "short string";
        assert_eq!(truncate_output(short, 100), short);

        let long = "a".repeat(300);
        let truncated = truncate_output(&long, 200);
        assert_eq!(truncated.len(), 203); // 200 chars + "..."
        assert!(truncated.ends_with("..."));
    }
}
