//! Dead code detection verification

use crate::verify::criteria::assertions::{evaluate, AssertionSpec};
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::{CommandConfinement, DeadCodeCheck};
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};

/// Default timeout for build commands that detect dead code (120 seconds)
const DEAD_CODE_TIMEOUT: Duration = Duration::from_secs(120);

/// Run dead code detection and parse output for violations
///
/// Executes the configured command (e.g., "cargo build --message-format=json"),
/// checks output against fail_patterns, and filters out false positives using
/// ignore_patterns.
///
/// `confinement` is the stage's resolved level for plan-authored commands.
///
/// Returns a Vec of VerificationGap for each violation found.
pub fn run_dead_code_check(
    check: &DeadCodeCheck,
    working_dir: &Path,
    confinement: CommandConfinement,
) -> Result<Vec<VerificationGap>> {
    run_with_timeout(check, working_dir, confinement, DEAD_CODE_TIMEOUT)
}

fn run_with_timeout(
    check: &DeadCodeCheck,
    working_dir: &Path,
    confinement: CommandConfinement,
    timeout: Duration,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    // Run the command and capture all output
    let spec = CommandSpec::shell(check.command.as_str());
    let result = run_spec_with_timeout(&spec, Some(working_dir), timeout, confinement)?;

    for failure in evaluate(&AssertionSpec::default(), &result).failures {
        gaps.push(VerificationGap::new(
            GapType::DeadCodeFound,
            format!("Dead code analysis unavailable: {failure}"),
            format!("Repair analyzer command: {}", check.command),
        ));
    }

    // Combine stdout and stderr for pattern matching
    let combined_output = format!("{}\n{}", result.stdout, result.stderr);

    for line in combined_output.lines().filter(|line| {
        check
            .fail_patterns
            .iter()
            .any(|pattern| line.contains(pattern))
            && !check
                .ignore_patterns
                .iter()
                .any(|pattern| line.contains(pattern))
    }) {
        gaps.push(VerificationGap::new(
            GapType::DeadCodeFound,
            format!("Dead code detected: {}", line.trim()),
            "Remove the unused code or add to ignore_patterns if intentional".to_string(),
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
        let result =
            run_dead_code_check(&check, &working_dir, CommandConfinement::default()).unwrap();

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
        let result =
            run_dead_code_check(&check, &working_dir, CommandConfinement::default()).unwrap();

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
        let result =
            run_dead_code_check(&check, &working_dir, CommandConfinement::default()).unwrap();

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
        let result =
            run_dead_code_check(&check, &working_dir, CommandConfinement::default()).unwrap();

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
        let result =
            run_dead_code_check(&check, &working_dir, CommandConfinement::default()).unwrap();

        assert!(result.is_empty(), "Expected no gaps for empty output");
    }
}

#[cfg(test)]
#[path = "dead_code_execution_tests.rs"]
mod execution_tests;
