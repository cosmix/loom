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
use crate::git::changes::{changes_since, merge_base};
use crate::git::worktree::WorktreeGit;
use crate::utils::sha256_hex;

pub(crate) use super::observer::{checkout_git, local_git};

pub(super) const MAX_REVIEW_FILE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeFingerprint {
    pub value: String,
    pub base: String,
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub behaviour: BTreeMap<String, String>,
}

/// `entries` are worktree-relative paths with content, or `None` for a deletion.
pub fn fingerprint_from(base: &str, entries: &[(String, Option<Vec<u8>>)]) -> ChangeFingerprint {
    let files: BTreeMap<String, String> = entries
        .iter()
        .map(|(path, content)| {
            let digest = content
                .as_ref()
                .map(|bytes| sha256_hex(bytes))
                .unwrap_or_else(|| "deleted".to_string());
            (path.clone(), digest)
        })
        .collect();

    ChangeFingerprint {
        value: fingerprint_value(base, &files),
        base: base.to_string(),
        files,
        behaviour: entries
            .iter()
            .filter_map(|(path, content)| {
                super::prose::behaviour_digest(path, content.as_deref())
                    .map(|digest| (path.clone(), digest))
            })
            .collect(),
    }
}

/// The fingerprint value of `base` and the per-path `files` digests.
pub(super) fn fingerprint_value(base: &str, files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"base:");
    hasher.update(base.as_bytes());
    hasher.update(b"\n");
    for (path, digest) in files {
        hasher.update(path.as_bytes());
        hasher.update(b"\t");
        hasher.update(digest.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
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
/// scaffolding and sandbox artifacts (`crate::git::changes::changes_since`).
/// Git runs through `repo`, read-only, without index writes or an fsmonitor
/// command; the daemon pins `repo` to the stage's
/// registered git directory ([`WorktreeGit::pinned`]) because it runs git in
/// a worktree an agent controls. The daemon's observer code calls this;
/// anything else calls [`compute`].
pub fn compute_local(repo: &WorktreeGit, target_branch: &str) -> Result<ChangeFingerprint> {
    let worktree = repo.work_tree();
    let base = merge_base(repo, "HEAD", target_branch)
        .with_context(|| format!("finding merge base with {target_branch}"))?;
    let paths = changes_since(repo, &base)?;
    let mut entries = Vec::new();
    for path in paths {
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
