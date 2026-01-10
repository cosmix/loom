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

    if failures.is_empty() {
        Ok(AcceptanceResult::AllPassed { results })
    } else {
        Ok(AcceptanceResult::Failed { results, failures })
    }
}
