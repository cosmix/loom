//! Shared ancestry verification for force-unsafe and merge-resolved paths.
//!
//! Both paths must refuse to mark a stage as merged unless the stage's commit
//! is in the target branch's history. This helper is **read-only**: it never
//! persists state. The caller is responsible for saving any newly derived
//! `completed_commit` only on the success path so refusal preserves stage file
//! state.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::git::branch::{branch_name_for_stage, get_branch_head};
use crate::git::merge::verify_merge_succeeded;
use crate::models::stage::Stage;

/// Result of `verify_or_derive_completed_commit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAncestry {
    /// The commit that was verified to be an ancestor of the target.
    pub commit: String,
    /// `Some(commit)` if the helper had to derive it from `loom/<id>` HEAD.
    /// Caller MUST persist by writing `stage.completed_commit = Some(commit)`
    /// and calling `save_stage` BEFORE proceeding. `None` if the stage already
    /// had `completed_commit` populated.
    pub persist_commit: Option<String>,
}

/// Read-only ancestry verification. Never mutates the stage; never saves.
///
/// Resolves the commit (from `stage.completed_commit` or by deriving it from
/// `loom/<id>` HEAD), then runs `verify_merge_succeeded` against the target
/// branch. Refusal returns `Err` so callers map to a refusal route.
///
/// # Arguments
/// * `stage` - the stage being verified (treated as immutable here)
/// * `target_branch` - the branch the merge should have produced (e.g., "main")
/// * `repo_root` - main repository root
pub fn verify_or_derive_completed_commit(
    stage: &Stage,
    target_branch: &str,
    repo_root: &Path,
) -> Result<VerifiedAncestry> {
    let (commit, persist_commit) = match &stage.completed_commit {
        Some(c) => (c.clone(), None),
        None => {
            let branch_name = branch_name_for_stage(&stage.id);
            let head = get_branch_head(&branch_name, repo_root).with_context(|| {
                format!(
                    "Cannot verify merge: stage '{}' has no completed_commit and \
                     branch {branch_name} does not exist",
                    stage.id
                )
            })?;
            (head.clone(), Some(head))
        }
    };

    if !verify_merge_succeeded(&commit, target_branch, repo_root)? {
        bail!(
            "Stage '{}' commit {} is not an ancestor of {}. \
             The merge has not actually happened. Either resolve the merge first \
             (loom stage merge {} --resolved after fixing conflicts) or omit this \
             completion path.",
            stage.id,
            commit,
            target_branch,
            stage.id
        );
    }
    Ok(VerifiedAncestry {
        commit,
        persist_commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus, StageType};
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(args: &[&str], cwd: &Path) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        run_git(&["init", "-b", "main"], root);
        run_git(&["config", "user.email", "t@t.com"], root);
        run_git(&["config", "user.name", "t"], root);
        std::fs::write(root.join("README.md"), "seed").unwrap();
        run_git(&["add", "README.md"], root);
        run_git(&["commit", "-m", "seed"], root);
        tmp
    }

    fn make_stage(id: &str) -> Stage {
        let mut s = Stage::new(id.to_string(), Some(format!("test {id}")));
        s.id = id.to_string();
        s.stage_type = StageType::Standard;
        s.status = StageStatus::Completed;
        s
    }

    #[test]
    fn verifies_when_completed_commit_is_ancestor_of_target() {
        let repo = init_repo();
        let root = repo.path();
        run_git(&["checkout", "-b", "loom/landed"], root);
        std::fs::write(root.join("a.rs"), "ok").unwrap();
        run_git(&["add", "a.rs"], root);
        run_git(&["commit", "-m", "landed"], root);
        run_git(&["checkout", "main"], root);
        run_git(&["merge", "--no-ff", "-m", "merge", "loom/landed"], root);

        let head = crate::git::branch::get_branch_head("loom/landed", root).unwrap();
        let mut stage = make_stage("landed");
        stage.completed_commit = Some(head.clone());

        let result = verify_or_derive_completed_commit(&stage, "main", root).unwrap();
        assert_eq!(result.commit, head);
        assert!(result.persist_commit.is_none());
    }

    #[test]
    fn refuses_when_completed_commit_not_ancestor_of_target() {
        let repo = init_repo();
        let root = repo.path();
        run_git(&["checkout", "-b", "loom/stranded"], root);
        std::fs::write(root.join("a.rs"), "ok").unwrap();
        run_git(&["add", "a.rs"], root);
        run_git(&["commit", "-m", "stranded"], root);
        run_git(&["checkout", "main"], root);

        let head = crate::git::branch::get_branch_head("loom/stranded", root).unwrap();
        let mut stage = make_stage("stranded");
        stage.completed_commit = Some(head);

        let result = verify_or_derive_completed_commit(&stage, "main", root);
        assert!(result.is_err());
    }

    #[test]
    fn derives_commit_from_branch_head_when_completed_commit_missing() {
        let repo = init_repo();
        let root = repo.path();
        run_git(&["checkout", "-b", "loom/derived"], root);
        std::fs::write(root.join("a.rs"), "ok").unwrap();
        run_git(&["add", "a.rs"], root);
        run_git(&["commit", "-m", "work"], root);
        run_git(&["checkout", "main"], root);
        run_git(&["merge", "--no-ff", "-m", "merge", "loom/derived"], root);

        let mut stage = make_stage("derived");
        stage.completed_commit = None;

        let result = verify_or_derive_completed_commit(&stage, "main", root).unwrap();
        assert!(result.persist_commit.is_some(), "must signal derivation");
        assert!(!result.commit.is_empty());
    }

    #[test]
    fn refuses_when_branch_missing_and_no_commit() {
        let repo = init_repo();
        let root = repo.path();

        let mut stage = make_stage("nope");
        stage.completed_commit = None;

        let result = verify_or_derive_completed_commit(&stage, "main", root);
        assert!(
            result.is_err(),
            "missing branch and missing commit must refuse"
        );
    }
}
