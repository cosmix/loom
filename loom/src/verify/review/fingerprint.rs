//! Content fingerprint for a review round's changes from its target branch.
//!
//! Every fingerprint that is recorded at one time and compared at another is
//! computed by one process, the loom daemon that owns the worktree (see
//! `observer`): [`compute`] asks it, and the daemon's own code calls
//! [`compute_local`] with git pinned to the stage's registered git directory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::observer::{self, DaemonUnreachable, Source};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::git::worktree::{is_worktree_scaffold_path, WorktreeGit};
use crate::verify::contracts::changes::{changed_names, git};
use crate::verify::tool_artifacts::is_tool_artifact;

pub(crate) use super::observer::local_git;

const MAX_REVIEW_FILE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeFingerprint {
    pub value: String,
    pub base: String,
    pub files: BTreeMap<String, String>,
}

/// `entries` are worktree-relative paths with content, or `None` for a deletion.
pub fn fingerprint_from(base: &str, entries: &[(String, Option<Vec<u8>>)]) -> ChangeFingerprint {
    let files: BTreeMap<String, String> = entries
        .iter()
        .map(|(path, content)| {
            let digest = content
                .as_ref()
                .map(|bytes| hex::encode(Sha256::digest(bytes)))
                .unwrap_or_else(|| "deleted".to_string());
            (path.clone(), digest)
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(b"base:");
    hasher.update(base.as_bytes());
    hasher.update(b"\n");
    for (path, digest) in &files {
        hasher.update(path.as_bytes());
        hasher.update(b"\t");
        hasher.update(digest.as_bytes());
        hasher.update(b"\n");
    }

    ChangeFingerprint {
        value: format!("sha256:{}", hex::encode(hasher.finalize())),
        base: base.to_string(),
        files,
    }
}

/// The fingerprint of `worktree` against `git merge-base HEAD
/// <target_branch>`, as the loom daemon that owns the worktree computes it.
///
/// A stage worktree (`<project>/.worktrees/<stage-id>`) is fingerprinted by
/// its project's daemon, asked over the project's socket: the fingerprint a
/// review round records and the one completion compares it against come from
/// one process, one environment and one filesystem view, whatever sandbox the
/// caller runs in. The daemon resolves the worktree and the target branch
/// from the stage; one that measured another target than `target_branch` is
/// an error.
///
/// This process computes the fingerprint itself ([`compute_local`]) only
/// when `worktree` is no stage worktree, or when nothing answers on the
/// socket and the project's daemon singleton lock proves that no daemon runs
/// (`observer`). Otherwise it gets the typed `DaemonUnreachable` error, and
/// a daemon's refusal is an error too; neither falls back to this process's
/// view.
pub fn compute(worktree: &Path, target_branch: &str) -> Result<ChangeFingerprint> {
    match observer::source(worktree)? {
        Source::ThisProcess(repo) => compute_local(&repo, target_branch),
        Source::Daemon {
            target_branch: measured,
            fingerprint,
        } => {
            ensure!(
                measured == target_branch,
                "the loom daemon measures this stage's changes against '{measured}', not \
                 '{target_branch}'"
            );
            Ok(fingerprint)
        }
    }
}

/// [`compute`], or, when no daemon answers and none is proven absent
/// (`DaemonUnreachable`), [`compute_local`] with a note saying the value is
/// this process's own view and can differ from the one completion uses. For
/// display, and for evidence the daemon derives again itself; never for a
/// value that is recorded or compared.
pub fn compute_or_local(
    worktree: &Path,
    target_branch: &str,
) -> Result<(ChangeFingerprint, Option<String>)> {
    match compute(worktree, target_branch) {
        Ok(fingerprint) => Ok((fingerprint, None)),
        Err(error) if error.is::<DaemonUnreachable>() => {
            let fingerprint = compute_local(&local_git(worktree)?, target_branch)?;
            let note = format!(
                "{error}; this fingerprint is this process's own view and can differ from the \
                 daemon's, which completion uses"
            );
            Ok((fingerprint, Some(note)))
        }
        Err(error) => Err(error),
    }
}

/// The fingerprint of `repo`'s worktree against `git merge-base HEAD
/// <target_branch>` in this process's own view: every path changed since the
/// base, plus every untracked file git does not ignore, less worktree
/// scaffolding and sandbox artifacts (`verify::tool_artifacts`). Git runs
/// through `repo`, read-only, without index writes or an fsmonitor command
/// (`contracts::changes::git`); the daemon pins `repo` to the stage's
/// registered git directory ([`WorktreeGit::pinned`]) because it runs git in
/// a worktree an agent controls. The daemon's observer code calls this;
/// anything else calls [`compute`].
pub fn compute_local(repo: &WorktreeGit, target_branch: &str) -> Result<ChangeFingerprint> {
    let worktree = repo.work_tree();
    let merge_base = git(repo, &["merge-base", "HEAD", target_branch])
        .with_context(|| format!("finding merge base with {target_branch}"))?;
    let base = String::from_utf8_lossy(&merge_base).trim().to_string();
    let mut paths = nul_paths(&changed_names(repo, &base, None)?)?;
    let untracked = nul_paths(&git(
        repo,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?)?;
    paths.extend(
        untracked
            .into_iter()
            .filter(|path| !is_tool_artifact(worktree, path)),
    );

    let mut entries = Vec::new();
    for path in paths {
        if is_worktree_scaffold_path(&path) {
            continue;
        }
        let content = match read_bounded(worktree, Path::new(&path), MAX_REVIEW_FILE_BYTES) {
            Ok(bytes) => Some(bytes),
            Err(error) if is_not_found(&error) => None,
            Err(error) => {
                return Err(error).with_context(|| format!("reading changed file {path}"));
            }
        };
        entries.push((path, content));
    }
    Ok(fingerprint_from(&base, &entries))
}

/// The NUL-separated paths git listed, each required to be UTF-8.
fn nul_paths(listed: &[u8]) -> Result<BTreeSet<String>> {
    listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .context("git returned a non-UTF-8 path")
                .map(str::to_owned)
        })
        .collect()
}

/// Sorted paths whose content hashes differ, including one-sided paths.
pub fn changed_since(
    previous: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Vec<String> {
    previous
        .keys()
        .chain(current.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| previous.get(*path) != current.get(*path))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "fingerprint_tests.rs"]
mod tests;
