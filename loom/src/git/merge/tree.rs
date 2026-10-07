//! Merge computed without touching the operator's main checkout.
//!
//! A merge is computed with `git merge-tree`, committed with
//! `git commit-tree`, and the target branch is advanced by `update-ref` (the
//! target is checked out nowhere) or `merge --ff-only` (checked out in the
//! main checkout, see [`super::checkout_apply`]). Every command runs through
//! `run_git`, which disables hooks and fsmonitor.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::git::branch::branch_ref;
use crate::git::runner::{run_git, run_git_checked};
use crate::git::signing;
use crate::git::target_guard::short;
use crate::git::worktree::list_worktrees;

/// Outcome of `git merge-tree --write-tree` for two commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeMerge {
    /// The merge is clean; `tree` is the merged tree object.
    Clean { tree: String },
    /// The merge conflicts in these paths (deduplicated, in git's order).
    Conflict { paths: Vec<String> },
}

/// Why the target branch was not advanced. Nothing in the main checkout was
/// changed when one of these is returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MergeBlock {
    /// The main checkout has an operator operation in progress.
    OperatorOperation { marker: String },
    /// The target branch is checked out in another worktree.
    TargetCheckedOutElsewhere { path: PathBuf },
    /// The target branch moved since the merge was computed.
    TargetMoved,
    /// Uncommitted work in the main checkout overlaps paths the merge touches.
    UncommittedOverlap { paths: Vec<String> },
    /// The fast-forward was refused for a reason the checks did not predict
    /// and git named no paths; `detail` is git's message on one line.
    FastForwardRefused { detail: String },
    /// The merge did not land and the operator's stashed changes could not
    /// be put back: they are in the top stash entry and in `backup_ref`.
    StashNotRestored { backup_ref: String },
    /// The target guard holds the target: it moved outside loom from
    /// `accepted` to `observed`, and merges into it wait for the operator.
    /// `accepted` is empty when the guard record could not be read, and
    /// `observed` when the live tip could not be read.
    TargetHeld {
        target: String,
        accepted: String,
        observed: String,
    },
}

impl fmt::Display for MergeBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OperatorOperation { marker } => write!(
                f,
                "the main checkout has an operation in progress ({marker}); finish or abort it first"
            ),
            Self::TargetCheckedOutElsewhere { path } => write!(
                f,
                "the target branch is checked out in another worktree ({}); switch that worktree to another branch",
                path.display()
            ),
            Self::TargetMoved => write!(
                f,
                "the target branch moved while the merge was being prepared; it will be retried"
            ),
            Self::UncommittedOverlap { paths } => write!(
                f,
                "uncommitted changes in the main checkout overlap the merge ({}); commit, stash or move them",
                paths.join(", ")
            ),
            Self::FastForwardRefused { detail } => write!(
                f,
                "git refused to fast-forward the main checkout: {detail}"
            ),
            Self::StashNotRestored { backup_ref } => write!(
                f,
                "the merge did not land and your uncommitted changes could not be put back: they are in the top `git stash list` entry and in {backup_ref}; restore them with `git stash pop`"
            ),
            Self::TargetHeld {
                target,
                accepted,
                observed,
            } => fmt_target_held(f, target, accepted, observed),
        }
    }
}

/// The operator text of [`MergeBlock::TargetHeld`]; an empty tip prints as
/// "unknown".
fn fmt_target_held(
    f: &mut fmt::Formatter<'_>,
    target: &str,
    accepted: &str,
    observed: &str,
) -> fmt::Result {
    if accepted.is_empty() {
        write!(
            f,
            "the target guard record could not be read, so target {target} is held at {}; \
             merges into it wait until the operator accepts its tip (loom target status)",
            short(observed)
        )
    } else {
        write!(
            f,
            "target {target} moved outside loom ({} → {}); merges into it wait until the \
             operator accepts or restores it (loom target status)",
            short(accepted),
            short(observed)
        )
    }
}

/// What happened to the operator's stashed changes around a merge that landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashReapply {
    /// Ref holding the stashed changes; kept after a restore as a backup.
    pub backup_ref: String,
    /// False when `stash pop --index` and `stash pop` both failed: the
    /// changes stay in the stash entry and in `backup_ref`.
    pub restored: bool,
}

impl StashReapply {
    /// One sentence for the operator: where the stashed changes are.
    pub fn notice(&self) -> String {
        let backup_ref = &self.backup_ref;
        if self.restored {
            format!(
                "uncommitted changes in the main checkout were stashed and reapplied around the merge; backup at {backup_ref}"
            )
        } else {
            format!(
                "the merge LANDED but your uncommitted changes in the main checkout could not be reapplied: they are in the top `git stash list` entry and in {backup_ref}; restore them with `git stash pop`"
            )
        }
    }
}

/// Result of [`advance_target`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advance {
    /// The target now points at the merge commit. `stash` is set when local
    /// changes were stashed around the fast-forward.
    Advanced { stash: Option<StashReapply> },
    /// The target was not advanced.
    Blocked(MergeBlock),
}

/// Compute the merge of `branch` into `target` (any revisions) without
/// touching a working tree or a ref.
pub fn merge_tree(repo: &Path, target: &str, branch: &str) -> Result<TreeMerge> {
    let args = [
        "merge-tree",
        "--write-tree",
        "--name-only",
        "--no-messages",
        "-z",
        target,
        branch,
    ];
    let output = run_git(&args, repo)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fields = stdout.split('\0').filter(|f| !f.is_empty());
    let code = output.status.code();
    // Exit 0 and 1 both print the tree first; a bad revision exits 1 with
    // nothing on stdout and is an error like any other exit code.
    let tree = match (code, fields.next()) {
        (Some(0 | 1), Some(tree)) => tree.to_string(),
        _ => bail!(
            "git merge-tree {target} {branch} failed (exit code {}): {}",
            code.map_or_else(|| "signal".to_string(), |c| c.to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    };
    if code == Some(0) {
        return Ok(TreeMerge::Clean { tree });
    }
    let mut paths: Vec<String> = Vec::new();
    for path in fields {
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
    }
    Ok(TreeMerge::Conflict { paths })
}

/// Write a merge commit for `tree` with the given parents; returns its id.
/// Signed when `commit.gpgsign` is true; a failure keeps its
/// [`CommitTreeError`](crate::git::signing::CommitTreeError) downcastable.
pub fn commit_merge(repo: &Path, tree: &str, parents: [&str; 2], message: &str) -> Result<String> {
    signing::commit_tree(repo, tree, &[parents[0], parents[1]], message)
        .map_err(anyhow::Error::from)
}

/// A computed merge whose commit is written only when an advance needs it,
/// so a blocked attempt leaves no unreachable commit behind.
pub struct PendingMerge<'a> {
    repo: &'a Path,
    tree: String,
    parents: [String; 2],
    message: String,
    commit: Option<String>,
}

impl<'a> PendingMerge<'a> {
    pub fn new(repo: &'a Path, tree: &str, parents: [&str; 2], message: &str) -> Self {
        Self {
            repo,
            tree: tree.to_string(),
            parents: parents.map(str::to_string),
            message: message.to_string(),
            commit: None,
        }
    }

    /// The merged tree.
    pub fn tree(&self) -> &str {
        &self.tree
    }

    /// The first parent: the target's tip the merge was computed against.
    pub fn old(&self) -> &str {
        &self.parents[0]
    }

    /// The merge commit, written on first use.
    pub fn commit(&mut self) -> Result<String> {
        if let Some(commit) = &self.commit {
            return Ok(commit.clone());
        }
        let [first, second] = &self.parents;
        let commit = commit_merge(self.repo, &self.tree, [first, second], &self.message)?;
        self.commit = Some(commit.clone());
        Ok(commit)
    }
}

/// Move `target` from the merge's first parent to its commit.
///
/// `repo` is the operator's main checkout. Where the target is checked out
/// decides the mechanism: nowhere, `update-ref` guarded by the old tip; in
/// `repo`, a fast-forward that keeps the operator's uncommitted work; in
/// another worktree, a block. The commit is written only on the paths that
/// can advance.
pub fn advance_target(
    repo: &Path,
    target: &str,
    stage_id: &str,
    pending: &mut PendingMerge,
) -> Result<Advance> {
    match checked_out_at(repo, target)? {
        Some(path) if same_path(&path, repo) => {
            super::checkout_apply::advance_in_checkout(repo, stage_id, pending)
        }
        Some(path) => Ok(Advance::Blocked(MergeBlock::TargetCheckedOutElsewhere {
            path,
        })),
        None => update_ref(repo, target, stage_id, pending),
    }
}

/// Path of the worktree that has `branch` checked out, if any. A worktree
/// whose directory is gone (deleted, not yet pruned) holds nothing: `update-ref`
/// is safe there.
fn checked_out_at(repo: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let reference = branch_ref(branch);
    Ok(list_worktrees(repo)?
        .into_iter()
        .filter(|wt| wt.branch.as_deref().map(branch_ref).as_ref() == Some(&reference))
        .find(|wt| !matches!(wt.path.try_exists(), Ok(false)))
        .map(|wt| wt.path))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn update_ref(
    repo: &Path,
    target: &str,
    stage_id: &str,
    pending: &mut PendingMerge,
) -> Result<Advance> {
    let (old, new) = (pending.old().to_string(), pending.commit()?);
    let reference = branch_ref(target);
    let reason = format!("loom: merge loom/{stage_id}");
    let output = run_git(&["update-ref", "-m", &reason, &reference, &new, &old], repo)?;
    if output.status.success() {
        return Ok(Advance::Advanced { stash: None });
    }
    if rev_parse(repo, &reference)? != old {
        return Ok(Advance::Blocked(MergeBlock::TargetMoved));
    }
    bail!(
        "git update-ref {reference} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Resolve `rev` to a commit id.
pub(crate) fn rev_parse(repo: &Path, rev: &str) -> Result<String> {
    run_git_checked(
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        repo,
    )
}

#[cfg(test)]
mod tests;
