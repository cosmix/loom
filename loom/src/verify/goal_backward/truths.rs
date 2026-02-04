//! Truth verification - observable behaviors that must work

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::TruthCheck;
use crate::verify::criteria::run_single_criterion_with_timeout;
use crate::verify::utils::truncate_string;

/// Default timeout for truth commands (30 seconds)
const TRUTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify all truth commands return exit code 0
pub fn verify_truths(truths: &[String], working_dir: &Path) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for truth in truths {
        let result = run_single_criterion_with_timeout(truth, Some(working_dir), TRUTH_TIMEOUT)?;

        if !result.success {
            let description = if result.timed_out {
                format!("Truth timed out: {truth}")
            } else {
                format!(
                    "Truth failed (exit {}): {}",
                    result
                        .exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    truth
                )
            };

            let suggestion = if !result.stderr.is_empty() {
                format!(
                    "Check error output: {}",
                    result.stderr.lines().next().unwrap_or("")
                )
            } else {
                "Verify the command works manually".to_string()
            };

            gaps.push(VerificationGap::new(
                GapType::TruthFailed,
                description,
                suggestion,
            ));
        }
    }

    Ok(gaps)
}

/// Verify enhanced truth checks with extended success criteria
pub fn verify_truth_checks(
    truth_checks: &[TruthCheck],
    working_dir: &Path,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for truth_check in truth_checks {
        let result = run_single_criterion_with_timeout(
            &truth_check.command,
            Some(working_dir),
            TRUTH_TIMEOUT,
        )?;

        // Check if timed out
        if result.timed_out {
            let description = match &truth_check.description {
                Some(desc) => format!("Truth check timed out: {}", desc),
                None => format!("Truth check timed out: {}", truth_check.command),
            };
            gaps.push(VerificationGap::new(
                GapType::TruthFailed,
                description,
                format!("Command exceeded {}s timeout", TRUTH_TIMEOUT.as_secs()),
            ));
            continue;
        }

        // Check exit code
        let expected_exit = truth_check.exit_code.unwrap_or(0);
        let actual_exit = result.exit_code.unwrap_or(-1);
        if actual_exit != expected_exit {
            let description = match &truth_check.description {
                Some(desc) => format!("Truth check failed: {}", desc),
                None => format!("Truth check failed: {}", truth_check.command),
            };
            let suggestion = format!(
                "Expected exit code {}, got {}. Command: {}",
                expected_exit, actual_exit, truth_check.command
            );
            gaps.push(VerificationGap::new(
                GapType::TruthFailed,
                description,
                suggestion,
            ));
            continue;
        }

        // Check stdout_contains patterns
        for pattern in &truth_check.stdout_contains {
            if !result.stdout.contains(pattern) {
                let description = match &truth_check.description {
                    Some(desc) => {
                        format!(
                            "Truth check failed: stdout missing expected pattern - {}",
                            desc
                        )
                    }
                    None => format!(
                        "Truth check failed: stdout missing expected pattern '{}'",
                        pattern
                    ),
                };
                let output_preview = truncate_string(&result.stdout, 200);
                let suggestion = format!(
                    "Command output: {}. Expected to contain: '{}'",
                    output_preview, pattern
                );
                gaps.push(VerificationGap::new(
                    GapType::TruthFailed,
                    description,
                    suggestion,
                ));
            }
        }

        // Check stdout_not_contains patterns
        for pattern in &truth_check.stdout_not_contains {
            if result.stdout.contains(pattern) {
                let description = match &truth_check.description {
                    Some(desc) => {
                        format!(
                            "Truth check failed: stdout contains forbidden pattern - {}",
                            desc
                        )
                    }
                    None => format!(
                        "Truth check failed: stdout contains forbidden pattern '{}'",
                        pattern
                    ),
                };
                let suggestion = format!("Remove or fix the code causing: '{}'", pattern);
                gaps.push(VerificationGap::new(
                    GapType::TruthFailed,
                    description,
                    suggestion,
                ));
            }
        }

        // Check stderr_empty if specified
        if let Some(true) = truth_check.stderr_empty {
            if !result.stderr.is_empty() {
                let description = match &truth_check.description {
                    Some(desc) => format!("Truth check failed: stderr was not empty - {}", desc),
                    None => "Truth check failed: stderr was not empty".to_string(),
                };
                let stderr_preview = truncate_string(&result.stderr, 200);
                let suggestion = format!("stderr output: {}", stderr_preview);
                gaps.push(VerificationGap::new(
                    GapType::TruthFailed,
                    description,
                    suggestion,
                ));
            }
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
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
        let result = verify_truth_checks(&checks, &working_dir).unwrap();
        assert!(result.is_empty(), "Expected no gaps when all criteria pass");
    }

    #[test]
    fn test_verify_truths_backward_compatibility() {
        let truths = vec!["echo 'test' && exit 0".to_string(), "true".to_string()];

        let working_dir = env::temp_dir();
        let result = verify_truths(&truths, &working_dir).unwrap();
        assert!(
            result.is_empty(),
            "Expected no gaps for successful simple truths"
        );
    }

    #[test]
    fn test_verify_truths_failure() {
        let truths = vec!["exit 1".to_string()];

        let working_dir = env::temp_dir();
        let result = verify_truths(&truths, &working_dir).unwrap();
        assert_eq!(result.len(), 1, "Expected one gap for failed truth");
        assert!(result[0].description.contains("Truth failed"));
    }

    // Tests for truncate_string moved to verify/utils.rs
}
