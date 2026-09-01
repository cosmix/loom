//! Before/after stage verification
//!
//! Verifies pre-conditions (before implementation) and post-conditions (after implementation)
//! using TruthCheck definitions from the plan.
//!
//! - Before-stage: Plan author writes TruthChecks describing the expected "before" state
//!   (e.g., exit_code: 1 for a test that should fail before the feature exists)
//! - After-stage: Plan author writes TruthChecks describing the expected "after" state
//!   (e.g., exit_code: 0 for a test that should pass after the feature is built)
//!
//! Both use verify_truth_checks() internally - the verification logic is identical.
//! The difference is semantic: when the checks run in the stage lifecycle.

use anyhow::Result;
use std::path::Path;

use crate::plan::schema::{CommandConfinement, TruthCheck};
use crate::verify::goal_backward::{verify_truth_checks, VerificationGap};

/// Run before-stage checks to verify pre-conditions.
///
/// Executes TruthChecks that describe the expected state BEFORE implementation.
/// If any check fails, the pre-conditions are not met and the stage should not proceed.
///
/// These run at the default (`Confined`) level: the orchestrator invokes them
/// before a stage is spawned, where no resolved sandbox config is in hand.
pub fn run_before_stage_checks(
    checks: &[TruthCheck],
    working_dir: &Path,
) -> Result<Vec<VerificationGap>> {
    verify_truth_checks(checks, working_dir, CommandConfinement::default())
}

/// Look for work a previous attempt at this stage already produced.
///
/// `before_stage` checks are delta-proofs: they assert the pre-condition that
/// the feature does NOT exist yet. That assertion is only meaningful while the
/// stage's workspace is pristine. Once an attempt has produced work, the checks
/// are *expected* to fail, and re-running them on a re-spawn (orphan recovery,
/// `loom stage retry`, crash retry) blocks the stage on its own progress —
/// permanently, because the stage is blocked before a session is ever spawned,
/// so nothing can carry the work forward.
///
/// Evidence, in order of cost: commits on the stage branch beyond the base it
/// was cut from, then any change in the worktree (including untracked files —
/// a newly added module is the common shape of "work already done"). Loom's own
/// worktree scaffolding is discounted: it is present from the first spawn, so
/// counting it would disable the gate entirely in repos that don't gitignore it.
///
/// Probe errors are treated as "no evidence" so a git failure keeps the gate's
/// existing behavior rather than silently disabling it.
///
/// # Arguments
/// * `stage_branch` - The stage's branch (`loom/<stage-id>`)
/// * `base_branch` - The base the stage's worktree was created from
/// * `repo_root` - Main repository root (where branch refs live)
/// * `worktree_path` - The stage's worktree root
///
/// # Returns
/// `Some(description)` when prior work exists (skip the gate), `None` when the
/// workspace is pristine (run the gate).
pub fn find_prior_stage_work(
    stage_branch: &str,
    base_branch: &str,
    repo_root: &Path,
    worktree_path: &Path,
) -> Option<String> {
    match crate::git::branch::commits_ahead_of(stage_branch, base_branch, repo_root) {
        Ok(count) if count > 0 => {
            return Some(format!(
                "{count} commit(s) on {stage_branch} beyond {base_branch}"
            ));
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                branch = %stage_branch,
                base = %base_branch,
                error = %e,
                "Could not count commits while checking for prior stage work"
            );
        }
    }

    match crate::git::branch::list_working_tree_changes(worktree_path) {
        Ok(changes) => {
            let changed: Vec<String> = changes
                .into_iter()
                .filter(|path| !crate::git::worktree::is_worktree_scaffold_path(path))
                .collect();
            let first = changed.first()?;
            Some(format!(
                "{} uncommitted change(s) in the worktree (e.g. {first})",
                changed.len()
            ))
        }
        Err(e) => {
            tracing::warn!(
                worktree = %worktree_path.display(),
                error = %e,
                "Could not read worktree status while checking for prior stage work"
            );
            None
        }
    }
}

/// Run after-stage checks to verify post-conditions.
///
/// Executes TruthChecks that describe the expected state AFTER implementation.
/// If any check fails, the post-conditions are not met and the stage completion should fail.
///
/// `confinement` is the stage's resolved level for plan-authored commands.
pub fn run_after_stage_checks(
    checks: &[TruthCheck],
    working_dir: &Path,
    confinement: CommandConfinement,
) -> Result<Vec<VerificationGap>> {
    verify_truth_checks(checks, working_dir, confinement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_before_after_stage_checks_pass() {
        // Simulate a "before" check: command exits 1 (feature doesn't exist yet)
        let before_checks = vec![TruthCheck {
            command: "exit 1".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(1),
            description: Some("Feature test should fail before implementation".to_string()),
        }];

        let working_dir = env::temp_dir();
        let gaps = run_before_stage_checks(&before_checks, &working_dir).unwrap();
        assert!(
            gaps.is_empty(),
            "Before-stage checks should pass when pre-conditions match"
        );
    }

    #[test]
    fn test_before_stage_checks_fail_when_precondition_not_met() {
        // Before check expects exit 1, but command exits 0 (feature already exists)
        let before_checks = vec![TruthCheck {
            command: "exit 0".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(1),
            description: Some("Feature should not exist yet".to_string()),
        }];

        let working_dir = env::temp_dir();
        let gaps = run_before_stage_checks(&before_checks, &working_dir).unwrap();
        assert_eq!(
            gaps.len(),
            1,
            "Should report gap when pre-condition not met"
        );
    }

    #[test]
    fn test_after_stage_checks_pass() {
        // Simulate an "after" check: command exits 0 (feature works)
        let after_checks = vec![TruthCheck {
            command: "echo 'feature works'".to_string(),
            stdout_contains: vec!["feature works".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: Some(0),
            description: Some("Feature should work after implementation".to_string()),
        }];

        let working_dir = env::temp_dir();
        let gaps =
            run_after_stage_checks(&after_checks, &working_dir, CommandConfinement::default())
                .unwrap();
        assert!(
            gaps.is_empty(),
            "After-stage checks should pass when post-conditions match"
        );
    }

    #[test]
    fn test_after_stage_checks_fail_when_postcondition_not_met() {
        // After check expects stdout to contain "feature works", but it doesn't
        let after_checks = vec![TruthCheck {
            command: "echo 'something else'".to_string(),
            stdout_contains: vec!["feature works".to_string()],
            stdout_not_contains: vec![],
            stderr_empty: None,
            exit_code: None,
            description: Some("Feature output check".to_string()),
        }];

        let working_dir = env::temp_dir();
        let gaps =
            run_after_stage_checks(&after_checks, &working_dir, CommandConfinement::default())
                .unwrap();
        assert_eq!(
            gaps.len(),
            1,
            "Should report gap when post-condition not met"
        );
    }

    #[test]
    fn test_before_after_empty_checks() {
        let working_dir = env::temp_dir();

        let gaps = run_before_stage_checks(&[], &working_dir).unwrap();
        assert!(
            gaps.is_empty(),
            "Empty before checks should produce no gaps"
        );

        let gaps =
            run_after_stage_checks(&[], &working_dir, CommandConfinement::default()).unwrap();
        assert!(gaps.is_empty(), "Empty after checks should produce no gaps");
    }

    #[test]
    fn test_before_stage_stdout_not_contains() {
        // Before check: stdout should NOT contain "FeatureX" (feature doesn't exist)
        let checks = vec![TruthCheck {
            command: "echo 'no features here'".to_string(),
            stdout_contains: vec![],
            stdout_not_contains: vec!["FeatureX".to_string()],
            stderr_empty: None,
            exit_code: None,
            description: Some("FeatureX should not appear before implementation".to_string()),
        }];

        let working_dir = env::temp_dir();
        let gaps = run_before_stage_checks(&checks, &working_dir).unwrap();
        assert!(
            gaps.is_empty(),
            "Check should pass when forbidden pattern is absent"
        );
    }

    #[test]
    fn test_after_stage_multiple_checks() {
        let checks = vec![
            TruthCheck {
                command: "echo 'test passed'".to_string(),
                stdout_contains: vec!["test passed".to_string()],
                stdout_not_contains: vec![],
                stderr_empty: None,
                exit_code: Some(0),
                description: Some("First post-condition".to_string()),
            },
            TruthCheck {
                command: "echo 'integration ok'".to_string(),
                stdout_contains: vec!["integration ok".to_string()],
                stdout_not_contains: vec![],
                stderr_empty: None,
                exit_code: Some(0),
                description: Some("Second post-condition".to_string()),
            },
        ];

        let working_dir = env::temp_dir();
        let gaps =
            run_after_stage_checks(&checks, &working_dir, CommandConfinement::default()).unwrap();
        assert!(gaps.is_empty(), "All after-stage checks should pass");
    }
}

#[cfg(test)]
mod prior_work_tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    const STAGE_BRANCH: &str = "loom/add-feature";

    fn git(args: &[&str], dir: &Path) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Repo with one commit; returns the temp dir and its base branch name.
    fn init_repo() -> (TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        git(&["init"], path);
        git(&["config", "user.email", "test@test.com"], path);
        git(&["config", "user.name", "Test"], path);
        std::fs::write(path.join("README.md"), "base").unwrap();
        git(&["add", "README.md"], path);
        git(&["commit", "-m", "Initial commit"], path);

        let head = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(path)
            .output()
            .unwrap();
        let base = String::from_utf8_lossy(&head.stdout).trim().to_string();

        (temp_dir, base)
    }

    #[test]
    fn test_no_prior_work_in_pristine_workspace() {
        let (temp_dir, base) = init_repo();
        let path = temp_dir.path();
        git(&["branch", STAGE_BRANCH], path);

        assert_eq!(
            find_prior_stage_work(STAGE_BRANCH, &base, path, path),
            None,
            "A pristine workspace must still run the before-stage gate"
        );
    }

    #[test]
    fn test_no_prior_work_when_stage_branch_absent() {
        let (temp_dir, base) = init_repo();
        let path = temp_dir.path();

        // First spawn: the branch does not exist yet.
        assert_eq!(find_prior_stage_work(STAGE_BRANCH, &base, path, path), None);
    }

    #[test]
    fn test_untracked_file_counts_as_prior_work() {
        let (temp_dir, base) = init_repo();
        let path = temp_dir.path();
        git(&["branch", STAGE_BRANCH], path);

        // The common shape of an interrupted attempt: a new module written but
        // never committed.
        std::fs::write(path.join("feature.rs"), "pub fn feature() {}").unwrap();

        let evidence = find_prior_stage_work(STAGE_BRANCH, &base, path, path)
            .expect("uncommitted work must be detected");
        assert!(
            evidence.contains("uncommitted"),
            "unexpected evidence: {evidence}"
        );
    }

    #[test]
    fn test_loom_worktree_scaffolding_is_not_prior_work() {
        let (temp_dir, base) = init_repo();
        let path = temp_dir.path();
        git(&["branch", STAGE_BRANCH], path);

        // What `create_worktree` plants in a repo that does not gitignore it.
        // Counting these would skip the gate on the very first spawn.
        std::fs::create_dir_all(path.join(".claude")).unwrap();
        std::fs::write(path.join(".claude/settings.local.json"), "{}").unwrap();
        std::fs::create_dir_all(path.join(".loom")).unwrap();
        std::fs::write(path.join(".loom/work"), "symlink stand-in").unwrap();
        std::fs::write(path.join("CLAUDE.md"), "project guidance").unwrap();

        assert_eq!(
            find_prior_stage_work(STAGE_BRANCH, &base, path, path),
            None,
            "loom's own worktree scaffolding must not read as prior work"
        );
    }

    #[test]
    fn test_commits_on_stage_branch_count_as_prior_work() {
        let (temp_dir, base) = init_repo();
        let path = temp_dir.path();

        git(&["checkout", "-b", STAGE_BRANCH], path);
        std::fs::write(path.join("feature.rs"), "pub fn feature() {}").unwrap();
        git(&["add", "feature.rs"], path);
        git(&["commit", "-m", "feat: add feature"], path);
        git(&["checkout", &base], path);

        // Working tree is clean again, but the commit is still on the branch.
        let evidence = find_prior_stage_work(STAGE_BRANCH, &base, path, path)
            .expect("committed work must be detected");
        assert!(
            evidence.contains("1 commit(s)"),
            "unexpected evidence: {evidence}"
        );
    }
}
