//! High-level acceptance criteria runner

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::config::CriteriaConfig;
use super::executor::run_single_criterion_with_timeout;
use super::result::AcceptanceResult;
use crate::models::stage::Stage;
use crate::verify::context::CriteriaContext;

/// Run all acceptance criteria for a stage with default configuration
///
/// This is a convenience wrapper around `run_acceptance_with_config` that uses
/// the default timeout settings.
pub fn run_acceptance(stage: &Stage, working_dir: Option<&Path>) -> Result<AcceptanceResult> {
    run_acceptance_with_config(stage, working_dir, &CriteriaConfig::default())
}

/// Run all acceptance criteria for a stage with custom configuration
///
/// Executes each shell command sequentially and collects results.
/// Returns AllPassed if all commands exit with code 0, Failed otherwise.
///
/// If `working_dir` is provided, commands will be executed in that directory.
/// This is typically used to run criteria in a worktree directory.
///
/// Context variables (like `${PROJECT_ROOT}`, `${WORKTREE}`) in criteria
/// are automatically expanded before execution.
///
/// If the stage has setup commands defined, they will be prepended to each
/// criterion command using `&&` to ensure environment preparation runs first.
///
/// Each command is subject to the timeout specified in `config`. Commands that
/// exceed the timeout are terminated and marked as failed.
pub fn run_acceptance_with_config(
    stage: &Stage,
    working_dir: Option<&Path>,
    config: &CriteriaConfig,
) -> Result<AcceptanceResult> {
    if stage.acceptance.is_empty() {
        return Ok(AcceptanceResult::AllPassed {
            results: Vec::new(),
        });
    }

    // Build context for variable expansion
    let default_dir = PathBuf::from(".");
    let ctx_path = working_dir.unwrap_or(&default_dir);
    let context = CriteriaContext::with_stage_id(ctx_path, &stage.id);

    let mut results = Vec::new();
    let mut failures = Vec::new();

    // Build setup prefix if setup commands are defined (also expand variables in setup)
    let setup_prefix = if stage.setup.is_empty() {
        None
    } else {
        let expanded_setup: Vec<String> = stage.setup.iter().map(|s| context.expand(s)).collect();
        Some(expanded_setup.join(" && "))
    };

    for command in &stage.acceptance {
        // Expand context variables in the command
        let expanded_command = context.expand(command);

        // Combine setup commands with criterion if setup is defined
        let full_command = match &setup_prefix {
            Some(prefix) => format!("{prefix} && {expanded_command}"),
            None => expanded_command,
        };

        let result =
            run_single_criterion_with_timeout(&full_command, working_dir, config.command_timeout)
                .with_context(|| format!("Failed to execute criterion: {command}"))?;

        if !result.success {
            let failure_reason = if result.timed_out {
                format!(
                    "Command '{}' timed out after {}s",
                    command,
                    config.command_timeout.as_secs()
                )
            } else {
                format!(
                    "Command '{}' failed with exit code {:?}",
                    command, result.exit_code
                )
            };
            failures.push(failure_reason);
        }

        // Store result with original command for cleaner output
        let mut result_with_original = result;
        result_with_original.command = command.clone();
        results.push(result_with_original);
    }

    // Advisory: warn about suspicious stderr patterns in successful commands
    for result in &results {
        for warning in detect_stderr_warnings(result) {
            eprintln!("warning: {}", warning);
        }
    }

    if failures.is_empty() {
        Ok(AcceptanceResult::AllPassed { results })
    } else {
        Ok(AcceptanceResult::Failed { results, failures })
    }
}

/// Detect suspicious patterns in stderr that may indicate silent failures.
/// Only checks results that reported success (exit code 0).
/// Returns a list of warning messages for each suspicious pattern found.
fn detect_stderr_warnings(result: &super::result::CriterionResult) -> Vec<String> {
    if !result.success || result.stderr.is_empty() {
        return Vec::new();
    }

    let patterns = [
        "connection refused",
        "permission denied",
        "failed to download",
        "blocked",
        "EACCES",
        "ECONNREFUSED",
        "unable to connect",
        "network error",
        "sandbox",
    ];

    let stderr_lower = result.stderr.to_lowercase();
    let mut warnings = Vec::new();

    for pattern in &patterns {
        let pattern_lower = pattern.to_lowercase();
        if stderr_lower.contains(&pattern_lower) {
            warnings.push(format!(
                "Command '{}' succeeded (exit 0) but stderr contains '{}' — may indicate a silent failure",
                result.command, pattern
            ));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::super::result::CriterionResult;
    use super::*;
    use std::time::Duration;

    fn make_result(success: bool, stderr: &str) -> CriterionResult {
        CriterionResult::new(
            "test-command".to_string(),
            success,
            String::new(),
            stderr.to_string(),
            if success { Some(0) } else { Some(1) },
            Duration::from_millis(100),
            false,
        )
    }

    #[test]
    fn test_detect_stderr_warnings_no_warnings_on_clean_stderr() {
        let result = make_result(true, "");
        assert!(detect_stderr_warnings(&result).is_empty());
    }

    #[test]
    fn test_detect_stderr_warnings_no_warnings_on_failure() {
        let result = make_result(false, "connection refused");
        assert!(detect_stderr_warnings(&result).is_empty());
    }

    #[test]
    fn test_detect_stderr_warnings_detects_connection_refused() {
        let result = make_result(true, "warning: connection refused to example.com");
        let warnings = detect_stderr_warnings(&result);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("connection refused"));
    }

    #[test]
    fn test_detect_stderr_warnings_detects_permission_denied() {
        let result = make_result(true, "error: Permission Denied when accessing /tmp/file");
        let warnings = detect_stderr_warnings(&result);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("permission denied"));
    }

    #[test]
    fn test_detect_stderr_warnings_case_insensitive() {
        let result = make_result(true, "BLOCKED by firewall");
        let warnings = detect_stderr_warnings(&result);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("blocked"));
    }

    #[test]
    fn test_detect_stderr_warnings_multiple_patterns() {
        let result = make_result(
            true,
            "blocked request, connection refused, sandbox restricted",
        );
        let warnings = detect_stderr_warnings(&result);
        assert_eq!(warnings.len(), 3); // blocked, connection refused, sandbox
    }

    #[test]
    fn test_detect_stderr_warnings_normal_stderr_no_match() {
        let result = make_result(true, "Compiling myproject v0.1.0\nFinished dev target");
        assert!(detect_stderr_warnings(&result).is_empty());
    }
}
