//! Whether a review round still covers the stage after its merge base moved
//! (DESIGN D12). A change fingerprint hashes its merge base, so a merge of the
//! target branch into the stage leaves no round's value matching, even when
//! the stage's changes are byte for byte the ones reviewed.
//!
//! After a base move the round covers the tree when every path the review
//! saw or the stage changes now is one of:
//!
//! - changed by the stage with the digest the review saw, and left alone by
//!   the target branch between the two bases: the stage's change to it is the
//!   reviewed one;
//! - no longer changed by the stage, with the new base holding exactly the
//!   content the review saw: the target branch carries the reviewed change.
//!
//! A path in neither change set is the target branch's own, brought in by the
//! base move. Anything else is stale: a changed digest, a path the review
//! never saw, a reviewed change dropped for the target's version, and reviewed
//! bytes kept over a target-branch change to the same path, which would revert
//! the target's work. A generated file is no exception: nothing the gate reads
//! proves that a differing bundle equals a build of reviewed sources.
//!
//! Only committed objects are read, through git pinned to the stage's
//! registered git directory: both bases are commits, so the result does not
//! depend on the reader's view of the worktree.

use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

use super::fingerprint::{fingerprint_value, nul_paths, ChangeFingerprint, MAX_REVIEW_FILE_BYTES};
use super::report::single_line;
use super::store::ReviewRound;
use crate::git::worktree::WorktreeGit;
use crate::models::stage::Stage;
use crate::stage_service::observer::stage_worktree_and_target;
use crate::verify::contracts::changes::{changed_names, git};

/// The paths that keep a round from covering the tree after a base move.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Unproven {
    /// Content the review did not see: a differing digest, a path the review
    /// never saw, or a reviewed change the new base does not carry.
    pub changed: Vec<String>,
    /// Reviewed bytes the stage keeps although the target branch changed the
    /// same path between the two bases.
    pub target_changed: Vec<String>,
}

impl Unproven {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.target_changed.is_empty()
    }

    /// The paths, for the gate's stale message.
    pub fn describe(&self) -> String {
        let list = |paths: &[String]| {
            let shown: Vec<String> = paths.iter().map(|path| single_line(path)).collect();
            shown.join(", ")
        };
        let mut parts = Vec::new();
        if !self.changed.is_empty() {
            parts.push(format!("changed since: {}", list(&self.changed)));
        }
        if !self.target_changed.is_empty() {
            parts.push(format!(
                "also changed on the target branch: {}",
                list(&self.target_changed)
            ));
        }
        parts.join("; ")
    }
}

/// Whether the merge base `current` measured differs from the one `latest`
/// recorded. A round without a base predates base recording and gets no
/// base-move allowance.
pub(super) fn moved(latest: &ReviewRound, current: &ChangeFingerprint) -> bool {
    !latest.base.is_empty() && !current.base.is_empty() && latest.base != current.base
}

/// [`unproven`] through git pinned to `stage`'s registered worktree.
pub(super) fn unproven_for_stage(
    stage: &Stage,
    work_dir: &Path,
    latest: &ReviewRound,
    current: &ChangeFingerprint,
) -> Result<Unproven> {
    let (repo, _) = stage_worktree_and_target(work_dir, stage)?;
    unproven(&repo, latest, current)
}

/// The paths of `latest` and `current` that the module rules leave
/// unproven, reading the two bases' committed trees through `repo`.
pub(super) fn unproven(
    repo: &WorktreeGit,
    latest: &ReviewRound,
    current: &ChangeFingerprint,
) -> Result<Unproven> {
    // The rules trust the round's per-path digests in place of its value, so
    // they must be the ones that value was computed from.
    ensure!(
        fingerprint_value(&latest.base, &latest.files) == latest.fingerprint,
        "review round {}'s recorded files do not hash to its fingerprint",
        latest.round
    );
    let (old, new) = (object_id(&latest.base)?, object_id(&current.base)?);
    let between = nul_paths(&changed_names(repo, old, Some(new))?)?;
    let mut unproven = Unproven::default();
    for (path, now) in &current.files {
        match latest.files.get(path) {
            Some(seen) if seen == now && between.contains(path) => {
                unproven.target_changed.push(path.clone());
            }
            Some(seen) if seen == now => {}
            _ => unproven.changed.push(path.clone()),
        }
    }
    for (path, seen) in &latest.files {
        if current.files.contains_key(path) {
            continue;
        }
        if !carried_by_base(repo, new, path, seen, &between)? {
            unproven.changed.push(path.clone());
        }
    }
    unproven.changed.sort();
    Ok(unproven)
}

/// Whether the new base `new` holds exactly the reviewed content `seen` at
/// `path`. A path the target branch did not change between the bases still
/// holds the old base's content, which the reviewed change differed from.
fn carried_by_base(
    repo: &WorktreeGit,
    new: &str,
    path: &str,
    seen: &str,
    between: &BTreeSet<String>,
) -> Result<bool> {
    if !between.contains(path) {
        return Ok(false);
    }
    Ok(committed_digest(repo, new, path)?.as_deref() == Some(seen))
}

/// The digest [`super::fingerprint::fingerprint_from`] would give `path` as
/// `commit` holds it: `deleted` when absent, the sha256 of a regular file's
/// blob, and `None` for anything no worktree read yields the same way (a
/// symlink, a submodule, a directory, a blob over the size limit).
fn committed_digest(repo: &WorktreeGit, commit: &str, path: &str) -> Result<Option<String>> {
    // ls-tree matches its paths literally: it supports no wildcard.
    let listed = git(
        repo,
        &["ls-tree", "-z", "-l", "--full-tree", commit, "--", path],
    )?;
    let Some(entry) = listed
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .find_map(|entry| entry.split_once('\t').filter(|(_, name)| *name == path))
    else {
        return Ok(Some("deleted".to_string()));
    };
    let fields: Vec<&str> = entry.0.split_whitespace().collect();
    let [mode, "blob", oid, size] = fields[..] else {
        return Ok(None);
    };
    let regular = matches!(mode, "100644" | "100755");
    let size: usize = size
        .parse()
        .with_context(|| format!("git listed a non-numeric size for {path} in {commit}"))?;
    if !regular || size > MAX_REVIEW_FILE_BYTES {
        return Ok(None);
    }
    let blob = git(repo, &["cat-file", "blob", oid])?;
    Ok(Some(hex::encode(Sha256::digest(&blob))))
}

/// `base` when it is a full hexadecimal object id, so it can never reach git
/// as an option or a revision expression.
fn object_id(base: &str) -> Result<&str> {
    ensure!(
        matches!(base.len(), 40 | 64) && base.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "the merge base {:?} is no full object id",
        single_line(base)
    );
    Ok(base)
}
