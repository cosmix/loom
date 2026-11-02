//! Preflight checks for `merge_retry`: resolves the stage ID, verifies the
//! stage and cwd are in a mergeable state, and refuses an in-progress merge —
//! all before the caller decides whether to spend a fix attempt on this run.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::git::merge::merge_head_exists;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, update_stage};

/// Everything a merge retry needs, resolved and validated up front.
pub(super) struct RetryPreflight {
    pub(super) stage_id: String,
    pub(super) repo_root: PathBuf,
    pub(super) worktree_root: PathBuf,
}

pub(super) enum RetryAdmission {
    Admitted(Stage),
    LimitReached(Stage),
}

/// Recheck and charge the current stage in the same locked transaction.
pub(super) fn admit_retry(stage_id: &str, work_dir: &Path) -> Result<RetryAdmission> {
    let mut limited = None;
    let updated = update_stage(stage_id, work_dir, |stage| {
        require_merge_state(stage, stage_id)?;
        if stage.is_at_fix_limit() {
            limited = Some(stage.clone());
            bail!("merge retry limit reached");
        }
        stage.increment_fix_attempts();
        Ok(())
    });
    if let Some(stage) = limited {
        return Ok(RetryAdmission::LimitReached(stage));
    }
    updated.map(RetryAdmission::Admitted)
}

/// Resolve the stage ID, load and validate the stage, confirm we're running
/// from a worktree, and refuse if a merge is already in progress.
///
/// This runs BEFORE the fix_attempts increment so a refused retry does not
/// burn an attempt.
pub(super) fn retry_preflight(stage_id: Option<String>, work_dir: &Path) -> Result<RetryPreflight> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    retry_preflight_from(stage_id, work_dir, &cwd)
}

fn retry_preflight_from(
    stage_id: Option<String>,
    work_dir: &Path,
    cwd: &Path,
) -> Result<RetryPreflight> {
    // Resolve stage ID: use provided or detect from current worktree branch
    let stage_id = super::resolve_stage_id(stage_id, "merge <stage-id>")?;

    let stage = load_stage(&stage_id, work_dir)?;
    require_merge_state(&stage, &stage_id)?;

    let (repo_root, worktree_root) = resolve_worktree_paths(&stage_id, cwd)?;

    Ok(RetryPreflight {
        stage_id,
        repo_root,
        worktree_root,
    })
}

/// Verify the stage is in a merge-failed state (MergeConflict, MergeBlocked,
/// or Completed but not yet merged).
fn require_merge_state(stage: &Stage, stage_id: &str) -> Result<()> {
    let is_merge_state = matches!(
        stage.status,
        StageStatus::MergeConflict | StageStatus::MergeBlocked
    ) || (stage.status == StageStatus::Completed && !stage.is_final_merged());

    if !is_merge_state {
        bail!(
            "Stage '{}' is in '{}' status. Only MergeConflict, MergeBlocked, or Completed but \
             not yet merged stages can use merge.\n\
             \n\
             Current status: {}\n\
             \n\
             For other failure states, use:\n\
             - loom stage retry {stage_id}       (for Blocked or CompletedWithFailures)\n\
             - loom stage merge {stage_id} --resolved (after manually resolving conflicts)",
            stage_id,
            stage.status,
            stage.status,
        );
    }

    Ok(())
}

/// Confirm we're running from a worktree, resolve the repo root and worktree
/// root, and refuse if a merge is already in progress in either.
///
/// Run BEFORE incrementing fix_attempts so a refused retry does not burn an
/// attempt.
fn resolve_worktree_paths(stage_id: &str, cwd: &Path) -> Result<(PathBuf, PathBuf)> {
    // Verify we're in a worktree (cwd should contain .worktrees in its path)
    let cwd_str = cwd.to_string_lossy();
    if !cwd_str.contains(".worktrees") {
        bail!(
            "merge must be run from within a worktree.\n\
             \n\
             Current directory: {}\n\
             \n\
             Navigate to the worktree first:\n\
             - cd .worktrees/{stage_id}",
            cwd.display(),
        );
    }

    // Find repo root (parent of .worktrees)
    let repo_root = super::find_repo_root(cwd)?;

    // Resolve worktree root from cwd. `git rev-parse --show-toplevel` returns
    // the top of the working tree, which for a worktree is its root.
    let worktree_root = crate::git::run_git_checked(&["rev-parse", "--show-toplevel"], cwd)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| cwd.to_path_buf());

    // Refuse if either main repo or the worktree has an active merge — running
    // a programmatic merge over an in-progress resolution would clobber the
    // user's work.
    let main_active = merge_head_exists(&repo_root)?;
    let worktree_active = merge_head_exists(&worktree_root)?;
    if main_active || worktree_active {
        bail!(
            "Cannot retry merge: a merge is already in progress (main: {main_active}, \
             worktree: {worktree_active}). Resolve or abort it first.",
        );
    }

    Ok((repo_root, worktree_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn retry_fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        git(root, &["init", "-b", "main"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        std::fs::write(root.join("seed"), "seed").unwrap();
        git(root, &["add", "seed"]);
        git(root, &["commit", "-m", "seed"]);
        git(
            root,
            &[
                "worktree",
                "add",
                "-b",
                "loom/stage-a",
                ".worktrees/stage-a",
            ],
        );
        let work_dir = root.join(".loom/work");
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.id = "stage-a".to_string();
        stage.status = StageStatus::MergeConflict;
        crate::verify::transitions::create_stage(&stage, &work_dir).unwrap();
        (temp, work_dir)
    }

    #[test]
    fn concurrent_retries_charge_only_the_last_available_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.id = "stage-a".to_string();
        stage.status = StageStatus::MergeConflict;
        stage.max_fix_attempts = Some(3);
        stage.fix_attempts = 2;
        crate::verify::transitions::create_stage(&stage, temp.path()).unwrap();
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let barrier = &barrier;
                    let path = temp.path();
                    scope.spawn(move || {
                        barrier.wait();
                        matches!(
                            admit_retry("stage-a", path).unwrap(),
                            RetryAdmission::Admitted(_)
                        )
                    })
                })
                .collect();
            barrier.wait();
            assert_eq!(
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .filter(|ok| *ok)
                    .count(),
                1
            );
        });
        assert_eq!(load_stage("stage-a", temp.path()).unwrap().fix_attempts, 3);
    }

    #[test]
    fn finalized_stage_is_not_charged() {
        let temp = tempfile::tempdir().unwrap();
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.id = "stage-a".to_string();
        stage.status = StageStatus::Completed;
        stage.merged = true;
        stage.fix_attempts = 1;
        crate::verify::transitions::create_stage(&stage, temp.path()).unwrap();
        assert!(admit_retry("stage-a", temp.path()).is_err());
        assert_eq!(load_stage("stage-a", temp.path()).unwrap().fix_attempts, 1);
    }

    #[test]
    fn final_transition_after_preflight_refuses_admission_without_charging() {
        let (repo, work_dir) = retry_fixture();
        let worktree = repo.path().join(".worktrees/stage-a");
        retry_preflight_from(Some("stage-a".to_string()), &work_dir, &worktree).unwrap();

        update_stage("stage-a", &work_dir, |stage| {
            stage.status = StageStatus::Completed;
            stage.merged = true;
            Ok(())
        })
        .unwrap();

        let error = admit_retry("stage-a", &work_dir)
            .err()
            .expect("a final stage must be refused");
        assert!(error.to_string().contains("Only MergeConflict"));
        let stage = load_stage("stage-a", &work_dir).unwrap();
        assert!(stage.is_final_merged());
        assert_eq!(stage.fix_attempts, 0);
    }

    #[test]
    fn changed_limit_after_preflight_controls_admission() {
        let (repo, work_dir) = retry_fixture();
        let worktree = repo.path().join(".worktrees/stage-a");
        retry_preflight_from(Some("stage-a".to_string()), &work_dir, &worktree).unwrap();

        update_stage("stage-a", &work_dir, |stage| {
            stage.fix_attempts = 1;
            stage.max_fix_attempts = Some(1);
            Ok(())
        })
        .unwrap();
        let RetryAdmission::LimitReached(limited) = admit_retry("stage-a", &work_dir).unwrap()
        else {
            panic!("the current cap must refuse admission");
        };
        assert_eq!(limited.get_effective_max_fix_attempts(), 1);
        assert_eq!(load_stage("stage-a", &work_dir).unwrap().fix_attempts, 1);

        update_stage("stage-a", &work_dir, |stage| {
            stage.max_fix_attempts = Some(3);
            Ok(())
        })
        .unwrap();
        let RetryAdmission::Admitted(admitted) = admit_retry("stage-a", &work_dir).unwrap() else {
            panic!("the current cap must permit admission");
        };
        assert_eq!(admitted.fix_attempts, 2);
        assert_eq!(admitted.get_effective_max_fix_attempts(), 3);
    }

    #[test]
    fn rejected_filesystem_preflight_does_not_charge() {
        let (repo, work_dir) = retry_fixture();
        let error = retry_preflight_from(Some("stage-a".to_string()), &work_dir, repo.path())
            .err()
            .expect("a main checkout is not a stage worktree");
        assert!(error.to_string().contains("worktree"));
        assert_eq!(load_stage("stage-a", &work_dir).unwrap().fix_attempts, 0);
    }

    #[test]
    fn admission_preserves_allowed_merge_states() {
        let temp = tempfile::tempdir().unwrap();
        for (id, status) in [
            ("conflict", StageStatus::MergeConflict),
            ("blocked", StageStatus::MergeBlocked),
            ("unmerged", StageStatus::Completed),
        ] {
            let mut stage = Stage::new(id.to_string(), None);
            stage.id = id.to_string();
            stage.status = status.clone();
            crate::verify::transitions::create_stage(&stage, temp.path()).unwrap();
            let RetryAdmission::Admitted(admitted) = admit_retry(id, temp.path()).unwrap() else {
                panic!("{id} should be admitted");
            };
            assert_eq!(admitted.status, status);
            assert_eq!(admitted.fix_attempts, 1);
        }
    }

    #[test]
    fn accepts_completed_stage_not_yet_merged() {
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.status = StageStatus::Completed;
        stage.merged = false;
        assert!(require_merge_state(&stage, "stage-a").is_ok());
    }

    #[test]
    fn refuses_completed_stage_already_merged() {
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.status = StageStatus::Completed;
        stage.merged = true;
        assert!(require_merge_state(&stage, "stage-a").is_err());
    }

    #[test]
    fn refuses_executing_stage() {
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.status = StageStatus::Executing;
        stage.merged = false;
        assert!(require_merge_state(&stage, "stage-a").is_err());
    }

    #[test]
    fn accepts_merge_blocked_stage() {
        let mut stage = Stage::new("stage-a".to_string(), None);
        stage.status = StageStatus::MergeBlocked;
        stage.merged = false;
        assert!(require_merge_state(&stage, "stage-a").is_ok());
    }
}
