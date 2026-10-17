//! Baseline comparison functionality
//!
//! Compares current test state against a captured baseline to detect
//! new failures, fixed failures, and warning changes.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use super::capture::{load_baseline, save_baseline};
use super::types::ChangeImpact;
use crate::plan::schema::{ChangeImpactConfig, CommandConfinement};
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};
use crate::verify::utils::extract_matching_lines;

/// Default timeout for comparison commands (5 minutes)
const COMPARE_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// Compare current state against a stored baseline.
///
/// Runs the comparison command (or baseline command if not specified),
/// extracts failures/warnings, and computes the diff against the baseline.
///
/// # Arguments
/// * `stage_id` - The stage ID to compare
/// * `config` - Change impact configuration
/// * `working_dir` - Directory to run commands from
/// * `tree` - Stage worktree root the commands may use in full
/// * `work_dir` - Path to .loom/work directory (for loading baseline)
/// * `confinement` - The stage's resolved level for plan-authored commands
///
/// # Returns
/// ChangeImpact describing what changed, or error if comparison fails
pub fn compare_to_baseline(
    stage_id: &str,
    config: &ChangeImpactConfig,
    working_dir: Option<&Path>,
    tree: Option<&Path>,
    work_dir: &Path,
    confinement: CommandConfinement,
) -> Result<ChangeImpact> {
    // Load the stored baseline
    let baseline = load_baseline(stage_id, work_dir)?
        .ok_or_else(|| anyhow::anyhow!("No baseline found for stage '{stage_id}'"))?;

    // Run comparison command (defaults to baseline command)
    let compare_command = config
        .compare_command
        .as_deref()
        .unwrap_or(&config.baseline_command);

    let spec = CommandSpec::shell(compare_command);
    let result = match run_spec_in_optional_tree(
        &spec,
        tree,
        working_dir,
        COMPARE_COMMAND_TIMEOUT,
        confinement,
    ) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Warning: Comparison command failed to run: {e}");
            return Ok(ChangeImpact::failed());
        }
    };

    // Combine stdout and stderr for pattern matching
    let combined_output = format!("{}\n{}", result.stdout, result.stderr);

    // Extract current failure lines
    let current_failures = extract_matching_lines(&combined_output, &config.failure_patterns)?;

    // Warning patterns are optional
    let warning_patterns: Vec<String> = Vec::new();
    let current_warnings = extract_matching_lines(&combined_output, &warning_patterns)?;

    let (new_failures, fixed_failures) = diff_lines(&baseline.failure_lines, &current_failures);
    let (new_warnings, fixed_warnings) = diff_lines(&baseline.warning_lines, &current_warnings);

    Ok(ChangeImpact::new(
        new_failures,
        fixed_failures,
        new_warnings,
        fixed_warnings,
        result.exit_code,
    ))
}

/// Lines only in `current` (new) and lines only in `baseline` (fixed).
fn diff_lines(baseline: &[String], current: &[String]) -> (Vec<String>, Vec<String>) {
    let baseline_set: HashSet<_> = baseline.iter().collect();
    let current_set: HashSet<_> = current.iter().collect();
    let new = current
        .iter()
        .filter(|l| !baseline_set.contains(l))
        .cloned()
        .collect();
    let fixed = baseline
        .iter()
        .filter(|l| !current_set.contains(l))
        .cloned()
        .collect();
    (new, fixed)
}

/// Capture baseline at stage start if not already captured.
///
/// This is the entry point for baseline capture during stage execution.
/// Returns Ok(()) if baseline already exists or was successfully captured.
///
/// # Arguments
/// * `stage_id` - The stage ID
/// * `config` - Change impact configuration
/// * `working_dir` - Directory to run commands from
/// * `tree` - Stage worktree root the commands may use in full
/// * `work_dir` - Path to .loom/work directory
/// * `confinement` - The stage's resolved level for plan-authored commands
pub fn ensure_baseline_captured(
    stage_id: &str,
    config: &ChangeImpactConfig,
    working_dir: Option<&Path>,
    tree: Option<&Path>,
    work_dir: &Path,
    confinement: CommandConfinement,
) -> Result<()> {
    // Check if baseline already exists
    if load_baseline(stage_id, work_dir)?.is_some() {
        return Ok(());
    }

    // Capture new baseline
    println!("Capturing baseline for change impact analysis...");
    let baseline =
        super::capture::capture_baseline(stage_id, config, working_dir, tree, confinement)?;

    println!(
        "  Baseline captured: {} failure(s), {} warning(s)",
        baseline.failure_count, baseline.warning_count
    );

    save_baseline(&baseline, work_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::baseline::types::TestBaseline;
    use tempfile::TempDir;

    fn create_test_baseline(stage_id: &str, failures: Vec<String>) -> TestBaseline {
        TestBaseline::new(
            stage_id,
            "cargo test",
            "test output",
            "",
            Some(0),
            failures,
            vec![],
        )
    }

    #[test]
    fn test_compare_finds_new_failures() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Save baseline with one failure
        let baseline =
            create_test_baseline("test-stage", vec!["FAILED: existing_test".to_string()]);
        save_baseline(&baseline, work_dir).unwrap();

        // Current state would have two failures (one new)
        // We can't easily test the full compare without mocking the command execution
        // but we can test the diff logic

        let baseline_failures: HashSet<_> = vec!["FAILED: existing_test".to_string()]
            .into_iter()
            .collect();
        let current_failures: HashSet<_> = vec![
            "FAILED: existing_test".to_string(),
            "FAILED: new_test".to_string(),
        ]
        .into_iter()
        .collect();

        let new: Vec<_> = current_failures
            .difference(&baseline_failures)
            .cloned()
            .collect();
        assert_eq!(new.len(), 1);
        assert!(new[0].contains("new_test"));
    }

    #[test]
    fn test_compare_finds_fixed_failures() {
        let baseline_failures: HashSet<_> =
            vec!["FAILED: test_a".to_string(), "FAILED: test_b".to_string()]
                .into_iter()
                .collect();
        let current_failures: HashSet<_> = vec!["FAILED: test_a".to_string()].into_iter().collect();

        let fixed: Vec<_> = baseline_failures
            .difference(&current_failures)
            .cloned()
            .collect();
        assert_eq!(fixed.len(), 1);
        assert!(fixed[0].contains("test_b"));
    }

    #[test]
    fn test_ensure_baseline_captured_skip_existing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Save a baseline
        let baseline = create_test_baseline("test-stage", vec![]);
        save_baseline(&baseline, work_dir).unwrap();

        // This should succeed without running the command
        let config = ChangeImpactConfig {
            baseline_command: "false".to_string(), // Would fail if run
            compare_command: None,
            failure_patterns: vec![],
            policy: crate::plan::schema::ChangeImpactPolicy::Fail,
        };

        let result = ensure_baseline_captured(
            "test-stage",
            &config,
            None,
            None,
            work_dir,
            CommandConfinement::default(),
        );
        assert!(result.is_ok());
    }
}
