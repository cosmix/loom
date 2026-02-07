//! Acceptance criteria directory resolution
//!
//! This module provides helpers for resolving the working directory
//! where acceptance criteria should be executed.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
use crate::models::stage::Stage;
use crate::verify::criteria::run_acceptance;

/// Resolved execution paths for a standard stage.
#[derive(Debug, Clone)]
pub(crate) struct StageExecutionPaths {
    /// Worktree root used for execution context.
    pub worktree_root: Option<PathBuf>,
    /// Final directory where acceptance criteria should run.
    pub acceptance_dir: Option<PathBuf>,
}

/// Display behavior for acceptance command output.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AcceptanceDisplayOptions<'a> {
    /// Optional label used in the start line (e.g. "stage", "knowledge stage").
    pub stage_label: Option<&'a str>,
    /// Whether to print an explicit message when no acceptance criteria are defined.
    pub show_empty_message: bool,
}

/// Resolve acceptance directory from worktree root and working_dir.
///
/// # Arguments
/// * `worktree_root` - The root of the worktree (e.g., ".worktrees/stage-id")
/// * `working_dir` - The stage's working_dir setting (e.g., ".", "loom", None)
///
/// # Returns
/// The resolved path for running acceptance criteria
pub(crate) fn resolve_acceptance_dir(
    worktree_root: Option<&Path>,
    working_dir: Option<&str>,
) -> Result<Option<PathBuf>> {
    match (worktree_root, working_dir) {
        (Some(root), Some(subdir)) => {
            // Handle "." special case - use worktree root directly
            if subdir == "." {
                return Ok(Some(root.to_path_buf()));
            }

            let full_path = root.join(subdir);

            // Canonicalize and check containment for path traversal defense
            let canonical = full_path.canonicalize().with_context(|| {
                let mut msg = format!(
                    "Failed to resolve acceptance directory: {} (working_dir='{}')",
                    full_path.display(),
                    subdir
                );
                // Check where build files actually are to provide hints
                for build_file in ["Cargo.toml", "package.json", "go.mod", "pyproject.toml"] {
                    if root.join(build_file).exists() {
                        msg.push_str(&format!(
                            "\n  HINT: {} found at worktree root — working_dir should probably be \".\"",
                            build_file
                        ));
                    }
                    // Also check common subdirectories
                    for subdir_name in ["loom", "app", "src", "packages"] {
                        if root.join(subdir_name).join(build_file).exists() && subdir != subdir_name {
                            msg.push_str(&format!(
                                "\n  HINT: {} found at {}/{} — working_dir should probably be \"{}\"",
                                build_file, subdir_name, build_file, subdir_name
                            ));
                        }
                    }
                }
                msg
            })?;

            // Defense-in-depth: verify resolved path is within worktree
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if !canonical.starts_with(&canonical_root) {
                anyhow::bail!(
                    "Acceptance directory {} escapes worktree root {}",
                    canonical.display(),
                    canonical_root.display()
                );
            }

            Ok(Some(canonical))
        }
        (Some(root), None) => {
            // No working_dir specified, use worktree root
            Ok(Some(root.to_path_buf()))
        }
        _ => Ok(None),
    }
}

/// Resolve both worktree root and acceptance directory for a standard stage.
pub(crate) fn resolve_stage_execution_paths(stage: &Stage) -> Result<StageExecutionPaths> {
    let expected_worktree_id = stage.worktree.as_deref();
    let cwd = std::env::current_dir().ok();
    let repo_root = cwd.as_ref().and_then(|p| find_repo_root_from_cwd(p));
    let cwd_worktree_root: Option<PathBuf> =
        cwd.as_ref().and_then(|p| find_worktree_root_from_cwd(p));
    let stage_worktree_root: Option<PathBuf> = stage
        .worktree
        .as_ref()
        .map(|w| {
            repo_root
                .as_ref()
                .map(|root| root.join(".worktrees").join(w))
                .unwrap_or_else(|| PathBuf::from(".worktrees").join(w))
        })
        .filter(|p| p.exists());

    // Prefer cwd worktree only when it matches the stage's assigned worktree.
    let worktree_root: Option<PathBuf> = match (cwd_worktree_root, stage_worktree_root) {
        (Some(cwd_root), Some(stage_root)) => {
            let cwd_stage_id = cwd_root.file_name().and_then(|s| s.to_str());
            if cwd_stage_id == expected_worktree_id {
                Some(cwd_root)
            } else {
                Some(stage_root)
            }
        }
        (Some(cwd_root), None) => Some(cwd_root),
        (None, Some(stage_root)) => Some(stage_root),
        (None, None) => None,
    };

    let acceptance_dir =
        resolve_acceptance_dir(worktree_root.as_deref(), stage.working_dir.as_deref())?;

    Ok(StageExecutionPaths {
        worktree_root,
        acceptance_dir,
    })
}

/// Resolve acceptance directory for knowledge stages in main repository context.
pub(crate) fn resolve_knowledge_acceptance_dir(stage: &Stage) -> Result<Option<PathBuf>> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or(cwd);

    resolve_acceptance_dir(Some(repo_root.as_path()), stage.working_dir.as_deref())
}

/// Run acceptance criteria and print standardized output.
///
/// Returns `true` when all criteria pass, `false` otherwise.
pub(crate) fn run_acceptance_with_display(
    stage: &Stage,
    stage_id: &str,
    acceptance_dir: Option<&Path>,
    options: AcceptanceDisplayOptions<'_>,
) -> Result<bool> {
    if stage.acceptance.is_empty() {
        if options.show_empty_message {
            println!("No acceptance criteria defined, treating as passed.");
        }
        return Ok(true);
    }

    if let Some(label) = options.stage_label {
        println!("Running acceptance criteria for {label} '{stage_id}'...");
    }
    if let Some(dir) = acceptance_dir {
        println!("  (working directory: {})", dir.display());
    }

    let result =
        run_acceptance(stage, acceptance_dir).context("Failed to run acceptance criteria")?;

    for criterion_result in result.results() {
        if criterion_result.success {
            println!("  ✓ passed: {}", criterion_result.command);
        } else if criterion_result.timed_out {
            println!("  ✗ TIMEOUT: {}", criterion_result.command);
        } else {
            println!("  ✗ FAILED: {}", criterion_result.command);
        }
    }

    if result.all_passed() {
        println!("All acceptance criteria passed!");
    }

    Ok(result.all_passed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_acceptance_dir_dot_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), Some(".")).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_subdir_uses_worktree_root_joined() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create the subdirectory
        let subdir_path = worktree_root.join("loom");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("loom")).unwrap();

        assert!(result.is_some());
        // Canonicalize for comparison
        let expected = subdir_path.canonicalize().unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_resolve_acceptance_dir_missing_subdir_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Don't create the subdirectory - should return error
        let result = resolve_acceptance_dir(Some(worktree_root), Some("nonexistent"));

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_acceptance_dir_none_working_dir_uses_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        let result = resolve_acceptance_dir(Some(worktree_root), None).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), worktree_root.to_path_buf());
    }

    #[test]
    fn test_resolve_acceptance_dir_no_worktree_returns_none() {
        let result = resolve_acceptance_dir(None, Some(".")).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_acceptance_dir_nested_subdir() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_root = temp_dir.path();

        // Create a nested subdirectory
        let subdir_path = worktree_root.join("packages/core");
        std::fs::create_dir_all(&subdir_path).unwrap();

        let result = resolve_acceptance_dir(Some(worktree_root), Some("packages/core")).unwrap();

        assert!(result.is_some());
        // Canonicalize for comparison
        let expected = subdir_path.canonicalize().unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    #[serial]
    fn test_resolve_stage_execution_paths_prefers_stage_worktree_when_cwd_mismatched() {
        let temp_dir = TempDir::new().unwrap();
        let expected_root = temp_dir.path().join(".worktrees/expected-stage");
        let other_root = temp_dir.path().join(".worktrees/other-stage");

        std::fs::create_dir_all(&expected_root).unwrap();
        std::fs::create_dir_all(&other_root).unwrap();

        let stage = Stage {
            worktree: Some("expected-stage".to_string()),
            ..Default::default()
        };

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&other_root).unwrap();

        let resolved = resolve_stage_execution_paths(&stage).unwrap();

        std::env::set_current_dir(original_dir).unwrap();

        assert_eq!(
            resolved.worktree_root.unwrap().canonicalize().unwrap(),
            expected_root.canonicalize().unwrap()
        );
    }

    #[test]
    #[serial]
    fn test_resolve_stage_execution_paths_uses_cwd_when_matching_stage_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let expected_root = temp_dir.path().join(".worktrees/expected-stage");
        std::fs::create_dir_all(&expected_root).unwrap();

        let stage = Stage {
            worktree: Some("expected-stage".to_string()),
            ..Default::default()
        };

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&expected_root).unwrap();

        let resolved = resolve_stage_execution_paths(&stage).unwrap();

        std::env::set_current_dir(original_dir).unwrap();

        assert_eq!(
            resolved.worktree_root.unwrap().canonicalize().unwrap(),
            expected_root.canonicalize().unwrap()
        );
    }
}
