//! Dead code detection verification

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::DeadCodeCheck;
use crate::verify::criteria::run_single_criterion_with_timeout;

/// Default timeout for build commands that detect dead code (120 seconds)
const DEAD_CODE_TIMEOUT: Duration = Duration::from_secs(120);

/// Run dead code detection and parse output for violations
///
/// Executes the configured command (e.g., "cargo build --message-format=json"),
/// checks output against fail_patterns, and filters out false positives using
/// ignore_patterns.
///
/// Returns a Vec of VerificationGap for each violation found.
pub fn run_dead_code_check(
    check: &DeadCodeCheck,
    working_dir: &Path,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    // Run the command and capture all output
    let result =
        run_single_criterion_with_timeout(&check.command, Some(working_dir), DEAD_CODE_TIMEOUT)?;

    // Even if the command fails, we should still check the output for patterns
    // Commands like cargo build may return non-zero with warnings

    // Combine stdout and stderr for pattern matching
    let combined_output = format!("{}\n{}", result.stdout, result.stderr);

    // Process each line of output
    for line in combined_output.lines() {
        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Check if any fail_pattern matches this line
        let mut has_fail_pattern = false;
        for fail_pattern in &check.fail_patterns {
            if line.contains(fail_pattern) {
                has_fail_pattern = true;
                break;
            }
        }

        // If no fail pattern matched, skip this line
        if !has_fail_pattern {
            continue;
        }

        // Check if any ignore_pattern matches (to filter false positives)
        let mut should_ignore = false;
        for ignore_pattern in &check.ignore_patterns {
            if line.contains(ignore_pattern) {
                should_ignore = true;
                break;
            }
        }

        // If this line should be ignored, skip it
        if should_ignore {
            continue;
        }

        // This is a violation - create a gap
        let description = format!("Dead code detected: {}", line.trim());
        let suggestion =
            "Remove the unused code or add to ignore_patterns if intentional".to_string();

        gaps.push(VerificationGap::new(
            GapType::DeadCodeFound,
            description,
            suggestion,
        ));
    }

    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_dead_code_detection_no_violations() {
        let check = DeadCodeCheck {
            command: "echo 'build successful'".to_string(),
            fail_patterns: vec!["warning: unused".to_string()],
            ignore_patterns: vec![],
        };

        let working_dir = env::current_dir().unwrap();
        let result = run_dead_code_check(&check, &working_dir).unwrap();

        assert!(
            result.is_empty(),
            "Expected no gaps when no fail patterns match"
        );
    }

    #[test]
    fn test_dead_code_detection_with_violations() {
        let check = DeadCodeCheck {
            command: "echo 'warning: unused function `old_helper`'".to_string(),
            fail_patterns: vec!["warning: unused".to_string()],
            ignore_patterns: vec![],
        };

        let working_dir = env::current_dir().unwrap();
        let result = run_dead_code_check(&check, &working_dir).unwrap();

        assert_eq!(result.len(), 1, "Expected one gap for the unused function");
        assert!(result[0]
            .description
            .contains("warning: unused function `old_helper`"));
    }

    #[test]
    fn test_dead_code_detection_with_ignore_patterns() {
        let check = DeadCodeCheck {
            command: "printf 'warning: unused function `old_helper`\\nwarning: unused function `allowed_unused_fn`'"
                .to_string(),
            fail_patterns: vec!["warning: unused".to_string()],
            ignore_patterns: vec!["allowed_unused_fn".to_string()],
        };

        let working_dir = env::current_dir().unwrap();
        let result = run_dead_code_check(&check, &working_dir).unwrap();

        assert_eq!(
            result.len(),
            1,
            "Expected one gap (second line should be ignored)"
        );
        assert!(result[0].description.contains("old_helper"));
        assert!(!result[0].description.contains("allowed_unused_fn"));
    }

    #[test]
    fn test_dead_code_detection_multiple_fail_patterns() {
        let check = DeadCodeCheck {
            command:
                "printf 'warning: unused function `fn1`\\nwarning: field `field1` is never read'"
                    .to_string(),
            fail_patterns: vec!["warning: unused".to_string(), "is never read".to_string()],
            ignore_patterns: vec![],
        };

        let working_dir = env::current_dir().unwrap();
        let result = run_dead_code_check(&check, &working_dir).unwrap();

        assert_eq!(result.len(), 2, "Expected two gaps for two violations");
    }

    #[test]
    fn test_dead_code_detection_empty_output() {
        let check = DeadCodeCheck {
            command: "echo ''".to_string(),
            fail_patterns: vec!["warning: unused".to_string()],
            ignore_patterns: vec![],
        };

        let working_dir = env::current_dir().unwrap();
        let result = run_dead_code_check(&check, &working_dir).unwrap();

        assert!(result.is_empty(), "Expected no gaps for empty output");
    }
}
