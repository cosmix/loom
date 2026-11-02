//! A fingerprint of everything a blocked merge's outcome depends on, so the
//! daemon can skip a retry that would only find the same block again. A
//! `merge_stage` attempt that reaches the stash dry run writes git objects
//! (the stash snapshot) before the target's advance is refused, so retrying an
//! unchanged block every tick fills the repository with unreachable objects.
//!
//! Computing the fingerprint writes nothing to the repository.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;

use super::checkout_files::{checked_stdout, porcelain_status};
use super::checkout_state::status_paths;
use super::in_progress::git_dir_for_repo_path;
use super::operator_operation;
use crate::git::branch::{branch_name_for_stage, branch_ref};
use crate::git::runner::run_git;

/// Hash of the inputs of an `UncommittedOverlap`, `FastForwardRefused`,
/// `TargetCheckedOutElsewhere` or `OperatorOperation` outcome of merging
/// `loom/<stage_id>` into `target_branch`: the tips of the target, the stage
/// branch and the main checkout's `HEAD`; the checkout's porcelain status and
/// the presence, size and modification time of every path it names and of
/// every `watched` path (an ignored file a block names is not in status); the
/// worktree list; the operator operation in progress; and whether the main
/// checkout's `index.lock` exists (an operator's tool holding it makes
/// `merge --ff-only` refuse without naming a path). The value lives in memory
/// only.
pub fn blocked_merge_inputs(
    repo_root: &Path,
    stage_id: &str,
    target_branch: &str,
    watched: &[String],
) -> Result<u64> {
    let mut hasher = DefaultHasher::new();
    let stage_branch = branch_name_for_stage(stage_id);
    for reference in [
        branch_ref(target_branch),
        branch_ref(&stage_branch),
        "HEAD".to_string(),
    ] {
        hash_tip(&mut hasher, repo_root, &reference)?;
    }

    let status = porcelain_status(repo_root)?;
    status.hash(&mut hasher);
    for path in status_paths(&String::from_utf8_lossy(&status)) {
        hash_file_stat(&mut hasher, &repo_root.join(&path));
    }
    for path in watched {
        hash_file_stat(&mut hasher, &repo_root.join(path));
    }

    checked_stdout(&["worktree", "list", "--porcelain"], &[], repo_root)?.hash(&mut hasher);
    operator_operation(repo_root)?.hash(&mut hasher);
    let index_lock = git_dir_for_repo_path(repo_root)?.join("index.lock");
    std::fs::symlink_metadata(index_lock)
        .is_ok()
        .hash(&mut hasher);
    Ok(hasher.finish())
}

/// Hash the commit `reference` resolves to, or the error text when it does not.
fn hash_tip(hasher: &mut DefaultHasher, repo_root: &Path, reference: &str) -> Result<()> {
    let output = run_git(&["rev-parse", "--verify", reference], repo_root)?;
    output.status.success().hash(hasher);
    output.stdout.hash(hasher);
    output.stderr.hash(hasher);
    Ok(())
}

/// Hash the length and modification time of `path`, or a marker when it is
/// missing. A symlink is not followed.
fn hash_file_stat(hasher: &mut DefaultHasher, path: &Path) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|since| since.as_nanos());
            (true, meta.len(), modified).hash(hasher);
        }
        Err(_) => (false, 0u64, None::<u128>).hash(hasher),
    }
}

#[cfg(test)]
mod tests;
