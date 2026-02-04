//! Baseline capture functionality
//!
//! Captures test state before stage execution begins, allowing comparison
//! after changes are made to detect regressions.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::types::TestBaseline;
use crate::plan::schema::ChangeImpactConfig;
use crate::verify::criteria::run_single_criterion_with_timeout;
use crate::verify::utils::extract_matching_lines;

/// Default timeout for baseline commands (5 minutes)
const BASELINE_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// Capture a baseline by running the configured command and extracting
/// failure/warning patterns.
///
/// # Arguments
/// * `stage_id` - The stage ID this baseline is for
/// * `config` - Change impact configuration with command and patterns
/// * `working_dir` - Directory to run the command from
///
/// # Returns
/// A TestBaseline containing the captured state
pub fn capture_baseline(
    stage_id: &str,
    config: &ChangeImpactConfig,
    working_dir: Option<&Path>,
) -> Result<TestBaseline> {
    let result = run_single_criterion_with_timeout(
        &config.baseline_command,
        working_dir,
        BASELINE_COMMAND_TIMEOUT,
    )
    .with_context(|| {
        format!(
            "Failed to run baseline command: {}",
            config.baseline_command
        )
    })?;

    // Combine stdout and stderr for pattern matching
    let combined_output = format!("{}\n{}", result.stdout, result.stderr);

    // Extract lines matching failure patterns
    let failure_lines = extract_matching_lines(&combined_output, &config.failure_patterns)?;

    // Warning patterns are optional - default to empty if not configured
    let warning_patterns: Vec<String> = Vec::new();
    let warning_lines = extract_matching_lines(&combined_output, &warning_patterns)?;

    Ok(TestBaseline::new(
        stage_id,
        &config.baseline_command,
        result.stdout,
        result.stderr,
        result.exit_code,
        failure_lines,
        warning_lines,
    ))
}

/// Save a baseline to the stage's directory in .work/stages/{stage-id}/
///
/// # Arguments
/// * `baseline` - The baseline to save
/// * `work_dir` - Path to the .work directory
pub fn save_baseline(baseline: &TestBaseline, work_dir: &Path) -> Result<()> {
    let stage_dir = work_dir.join("stages").join(&baseline.stage_id);
    fs::create_dir_all(&stage_dir)
        .with_context(|| format!("Failed to create stage directory: {}", stage_dir.display()))?;

    let baseline_path = stage_dir.join("baseline.json");
    let json =
        serde_json::to_string_pretty(baseline).context("Failed to serialize baseline to JSON")?;

    fs::write(&baseline_path, json)
        .with_context(|| format!("Failed to write baseline: {}", baseline_path.display()))?;

    Ok(())
}

/// Load a baseline from the stage's directory
///
/// # Arguments
/// * `stage_id` - The stage ID to load baseline for
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// Some(TestBaseline) if found, None if no baseline exists
pub fn load_baseline(stage_id: &str, work_dir: &Path) -> Result<Option<TestBaseline>> {
    let baseline_path = work_dir.join("stages").join(stage_id).join("baseline.json");

    if !baseline_path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(&baseline_path)
        .with_context(|| format!("Failed to read baseline: {}", baseline_path.display()))?;

    let baseline: TestBaseline =
        serde_json::from_str(&json).context("Failed to parse baseline JSON")?;

    Ok(Some(baseline))
}

/// Check if a baseline exists for a stage
pub fn baseline_exists(stage_id: &str, work_dir: &Path) -> bool {
    work_dir
        .join("stages")
        .join(stage_id)
        .join("baseline.json")
        .exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Tests for extract_matching_lines moved to verify/utils.rs

    #[test]
    fn test_save_and_load_baseline() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let baseline = TestBaseline::new(
            "test-stage",
            "cargo test",
            "stdout content",
            "stderr content",
            Some(0),
            vec!["FAILED: test".to_string()],
            vec![],
        );

        save_baseline(&baseline, work_dir).unwrap();

        let loaded = load_baseline("test-stage", work_dir).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.stage_id, "test-stage");
        assert_eq!(loaded.failure_count, 1);
    }

    #[test]
    fn test_load_baseline_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let loaded = load_baseline("nonexistent", work_dir).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_baseline_exists() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        assert!(!baseline_exists("test-stage", work_dir));

        let baseline = TestBaseline::new("test-stage", "cmd", "", "", Some(0), vec![], vec![]);
        save_baseline(&baseline, work_dir).unwrap();

        assert!(baseline_exists("test-stage", work_dir));
    }
}
