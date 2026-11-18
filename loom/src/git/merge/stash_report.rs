//! What a merge tells the operator about the tracked changes it stashed.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Where the operator's tracked changes are when a rollback stops incomplete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RollbackChanges {
    /// The merge made no stash and no copy of them.
    NoBackup,
    /// Never stashed: they stay in the checkout, with a copy in `backup_ref`.
    InCheckout { backup_ref: String },
    /// Stashed and popped: they are back in the checkout, with a copy in
    /// `backup_ref`.
    Restored { backup_ref: String },
    /// Stashed and not popped: they are in `backup_ref`, and in a stash entry
    /// unless something removed it.
    Stashed { backup_ref: String },
    /// Stashed, then the reset failed with the checkout as it was: they are in
    /// the checkout, and the stash entry and `backup_ref` are spare copies.
    ResetRefused { backup_ref: String },
}

impl RollbackChanges {
    pub(super) fn fmt_note(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBackup => Ok(()),
            Self::InCheckout { backup_ref } => write!(
                f,
                "; your uncommitted changes were not stashed and stay in the checkout, with a copy in {backup_ref}"
            ),
            Self::Restored { backup_ref } => write!(
                f,
                "; your uncommitted changes were put back in the checkout, with a copy in {backup_ref}"
            ),
            Self::Stashed { backup_ref } => write!(
                f,
                "; your uncommitted changes remain in {backup_ref} and, unless it was removed, in a `git stash list` entry; {}",
                restore_remedy(backup_ref)
            ),
            Self::ResetRefused { backup_ref } => write!(
                f,
                "; your uncommitted changes are still in the checkout; {}",
                spare_copy_remedy(backup_ref)
            ),
        }
    }
}

/// How to restore changes held in `backup_ref`. `git stash pop` would take the top
/// entry, and `refs/stash` is shared with every worktree of the repository.
pub(super) fn restore_remedy(backup_ref: &str) -> String {
    format!(
        "restore them with `git stash apply {backup_ref}`, then drop the entry from `git stash list` and delete the ref with `git update-ref -d {backup_ref}`"
    )
}

/// How to remove the copies of changes the checkout still holds.
pub(super) fn spare_copy_remedy(backup_ref: &str) -> String {
    format!(
        "the stash entry and {backup_ref} are spare copies: drop the entry from `git stash list` and delete the ref with `git update-ref -d {backup_ref}`"
    )
}

/// What happened to the operator's stashed changes around a merge that landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashReapply {
    /// Ref holding the stashed changes; kept after a restore as a backup.
    pub backup_ref: String,
    /// False when the merge's stash entry was gone, or `stash apply --index`
    /// and `stash apply` both failed: the changes stay in `backup_ref`, and in
    /// the entry if it exists.
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
                "the merge LANDED but your uncommitted changes in the main checkout could not be reapplied: they are in {backup_ref} and, unless it was removed, in a `git stash list` entry; {}",
                restore_remedy(backup_ref)
            )
        }
    }
}
