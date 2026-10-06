//! The commit the daemon writes for a session: its staged index, committed
//! with plumbing only.
//!
//! A stage session runs sandboxed, and the signing setup (`~/.gnupg`, the
//! agent sockets) is outside its sandbox, so the session stages its files and
//! asks the daemon to commit them. [`Committer::commit_staged`] checks what
//! the session saw (HEAD, the staged tree) against the worktree, refuses state
//! paths and gitlinks, writes the commit with `commit-tree` (signed when
//! `commit.gpgsign` is true) and moves the branch with a compare-and-swap
//! `update-ref`. Every git call goes through the runner, so no repository
//! hook runs, and every index and ref read goes through the given
//! [`WorktreeGit`], so the daemon pins it to the worktree's registered
//! administrative directory. The merge lock and the target-guard attestation
//! are the caller's: this module has no state directory.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::git::branch::{branch_name_for_stage, branch_ref};
use crate::git::signing::{self, CommitTreeError};
use crate::git::worktree::WorktreeGit;
use crate::validation::validate_id;

mod checks;

use checks::require_object_id;
pub use checks::validate_commit_message;

/// What the session saw when it asked for the commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRequest {
    pub message: String,
    /// The commit HEAD named; the branch must still point at it.
    pub expected_head: String,
    /// `git write-tree` of the staged index; the index must still write it.
    pub expected_tree: String,
}

/// Which branch the commit lands on, and what it may carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitScope {
    /// `loom/<stage_id>` in the stage's worktree.
    StageBranch { stage_id: String },
    /// `target_branch`, with every changed path under `prefix`.
    Knowledge {
        target_branch: String,
        prefix: PathBuf,
    },
    /// `loom/<stage_id>` with a merge in progress: `MERGE_HEAD` becomes the
    /// second parent.
    Merge { stage_id: String },
}

/// Why no commit was written. The branch is unmoved in every case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitRefusal {
    /// The signer failed; only the operator can fix it.
    Signing { detail: String },
    /// A check failed or git refused.
    Refused { reason: String },
}

impl fmt::Display for CommitRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signing { detail } => write!(f, "signing failed: {detail}"),
            Self::Refused { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for CommitRefusal {}

fn refused(reason: impl Into<String>) -> CommitRefusal {
    CommitRefusal::Refused {
        reason: reason.into(),
    }
}

/// The full ref the commit moves.
fn scope_ref(scope: &CommitScope) -> Result<String, CommitRefusal> {
    match scope {
        CommitScope::StageBranch { stage_id } | CommitScope::Merge { stage_id } => {
            validate_id(stage_id).map_err(|error| refused(format!("bad stage id: {error}")))?;
            Ok(branch_ref(&branch_name_for_stage(stage_id)))
        }
        CommitScope::Knowledge {
            target_branch,
            prefix,
        } => {
            if target_branch.is_empty() || target_branch.starts_with('-') {
                return Err(refused(format!("bad target branch {target_branch:?}")));
            }
            if prefix.as_os_str().is_empty() || prefix.is_absolute() {
                return Err(refused(format!(
                    "bad knowledge prefix {}",
                    prefix.display()
                )));
            }
            Ok(format!("refs/heads/{target_branch}"))
        }
    }
}

/// Commits a worktree's staged index for the daemon.
pub struct Committer<'a> {
    git: &'a WorktreeGit,
    repo_root: &'a Path,
    merge_target: Option<&'a str>,
}

impl<'a> Committer<'a> {
    /// `git` runs every index and ref command; `repo_root` is the main
    /// repository, whose configuration signs the commit.
    pub fn new(git: &'a WorktreeGit, repo_root: &'a Path) -> Self {
        Self {
            git,
            repo_root,
            merge_target: None,
        }
    }

    /// Require a Merge scope's `MERGE_HEAD` to be `target_branch`'s tip or
    /// an ancestor of it. A session can write `MERGE_HEAD`, and the path
    /// checks pass a gitlink or state path that `MERGE_HEAD`'s tree holds, so
    /// the daemon names the target here; it is checked on the same id the
    /// commit records as its second parent.
    pub fn merging_into(mut self, target_branch: &'a str) -> Self {
        self.merge_target = Some(target_branch);
        self
    }

    /// Commit the staged index on the scope's branch; returns the new
    /// commit id. Each check refuses naming what it saw, and nothing moves
    /// until the final compare-and-swap `update-ref`.
    pub fn commit_staged(
        &self,
        scope: &CommitScope,
        request: &CommitRequest,
    ) -> Result<String, CommitRefusal> {
        validate_commit_message(&request.message).map_err(refused)?;
        require_object_id("expected_head", &request.expected_head)?;
        require_object_id("expected_tree", &request.expected_tree)?;
        let reference = scope_ref(scope)?;
        self.require_branch(&reference)?;
        let head = self.require_head(&request.expected_head)?;
        let merge_head = self.merge_head(scope)?;
        if !self.checked(&["ls-files", "--unmerged", "-z"])?.is_empty() {
            return Err(refused(
                "the index has unmerged paths; resolve and stage them",
            ));
        }
        let tree = self.require_tree(&request.expected_tree)?;
        self.require_allowed_paths(scope, &head, merge_head.as_deref(), &tree)?;
        let mut parents = vec![head.as_str()];
        parents.extend(merge_head.as_deref());
        let new = signing::commit_tree(self.repo_root, &tree, &parents, &request.message)
            .map_err(commit_tree_refusal)?;
        self.move_ref(&reference, &new, &head, &request.message)?;
        if merge_head.is_some() {
            self.quit_merge();
        }
        Ok(new)
    }

    /// `git <args>`, refusing when git cannot be started.
    fn run(&self, args: &[&str]) -> Result<Output, CommitRefusal> {
        self.git
            .run(args)
            .map_err(|error| refused(format!("git {} could not run: {error:#}", args[0])))
    }

    /// Stdout of a `git <args>` that must succeed, untrimmed.
    fn stdout(&self, args: &[&str]) -> Result<String, CommitRefusal> {
        let output = self.run(args)?;
        if !output.status.success() {
            return Err(git_failure(args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Trimmed stdout of a `git <args>` that must succeed.
    fn checked(&self, args: &[&str]) -> Result<String, CommitRefusal> {
        Ok(self.stdout(args)?.trim().to_string())
    }

    /// HEAD is a symbolic ref to exactly `reference`.
    fn require_branch(&self, reference: &str) -> Result<(), CommitRefusal> {
        let args = ["symbolic-ref", "--quiet", "HEAD"];
        let output = self.run(&args)?;
        let named = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match output.status.code() {
            Some(0) if named == reference => Ok(()),
            Some(0) => Err(refused(format!(
                "HEAD is on {named}; this commit belongs on {reference}"
            ))),
            Some(1) => Err(refused(format!(
                "HEAD is detached; this commit belongs on {reference}"
            ))),
            _ => Err(git_failure(&args, &output)),
        }
    }

    fn require_head(&self, expected: &str) -> Result<String, CommitRefusal> {
        let head = self.checked(&["rev-parse", "--verify", "HEAD"])?;
        if head != expected {
            return Err(refused(format!(
                "HEAD moved: it is {head}, the request saw {expected}; stage again and retry"
            )));
        }
        Ok(head)
    }

    /// `MERGE_HEAD`, required in a Merge scope and refused in any other. In a
    /// Merge scope it names one commit, on the merge target when one was
    /// given ([`Self::merging_into`]).
    fn merge_head(&self, scope: &CommitScope) -> Result<Option<String>, CommitRefusal> {
        let args = ["rev-parse", "--verify", "--quiet", "MERGE_HEAD^{commit}"];
        let output = self.run(&args)?;
        let merge_head = match output.status.code() {
            Some(0) => Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            Some(1) => None,
            _ => return Err(git_failure(&args, &output)),
        };
        match (matches!(scope, CommitScope::Merge { .. }), merge_head) {
            (true, None) => Err(refused(
                "a merge commit needs MERGE_HEAD: run git merge --no-commit --no-ff <target> first",
            )),
            (false, Some(id)) => Err(refused(format!(
                "a merge is in progress (MERGE_HEAD {id}); only a merge session can finish it: \
                 run git merge --abort"
            ))),
            (true, Some(id)) => {
                self.require_single_merge_head()?;
                self.require_merged_target(&id)?;
                Ok(Some(id))
            }
            (false, None) => Ok(None),
        }
    }

    fn require_tree(&self, expected: &str) -> Result<String, CommitRefusal> {
        let tree = self.checked(&["write-tree"])?;
        if tree != expected {
            return Err(refused(format!(
                "the staged tree is {tree}, the request saw {expected}: the index changed; \
                 stage again and retry"
            )));
        }
        Ok(tree)
    }

    /// Move `reference` from `old` to `new`, only if it still names `old`.
    /// `--no-deref`: a symbolic ref planted at the branch is replaced, never
    /// followed to the ref it names.
    fn move_ref(
        &self,
        reference: &str,
        new: &str,
        old: &str,
        message: &str,
    ) -> Result<(), CommitRefusal> {
        let subject = message.lines().next().unwrap_or_default().trim();
        let reason = format!("loom: commit {subject}");
        let args = [
            "update-ref",
            "--no-deref",
            "-m",
            &reason,
            reference,
            new,
            old,
        ];
        let output = self.run(&args)?;
        if !output.status.success() {
            return Err(refused(format!(
                "git update-ref {reference} refused; the branch is unmoved: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    /// Clear the merge state after a merge commit. The branch already moved,
    /// so a failure is logged, not refused.
    fn quit_merge(&self) {
        match self.git.run(&["merge", "--quit"]) {
            Ok(output) if output.status.success() => {}
            Ok(output) => tracing::warn!(
                "git merge --quit failed after the merge commit: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(error) => tracing::warn!("git merge --quit could not run: {error:#}"),
        }
    }
}

fn commit_tree_refusal(error: CommitTreeError) -> CommitRefusal {
    if error.signing {
        CommitRefusal::Signing {
            detail: error.detail,
        }
    } else {
        refused(error.to_string())
    }
}

fn git_failure(args: &[&str], output: &Output) -> CommitRefusal {
    refused(format!(
        "git {} failed ({}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// [`Committer::commit_staged`] with git discovered from `repo`, which also
/// signs: for a process that already runs with the privileges of whoever
/// controls `repo`.
pub fn commit_staged(
    repo: &Path,
    scope: &CommitScope,
    request: &CommitRequest,
) -> Result<String, CommitRefusal> {
    Committer::new(&WorktreeGit::discovered(repo), repo).commit_staged(scope, request)
}

#[cfg(test)]
mod tests;
