//! Finds an earlier merge attempt's quarantine that still holds a stage's
//! files. While one exists no merge of that stage may land: the paths it
//! emptied would take the branch's bytes, and the block that points at the
//! quarantine would be cleared.

use anyhow::{anyhow, Result};
use std::io;
use std::path::{Path, PathBuf};

use super::displace::QUARANTINE_ROOT;
use crate::git::merge::in_progress::git_dir_for_repo_path;
use crate::git::merge::tree::MergeBlock;

const HELD: &str = "files from an earlier merge attempt are still in the quarantine; \
move each back to its path in the checkout, then delete the quarantine directory and retry";

/// The block for a merge of `stage_id` while one of its quarantine
/// directories exists under the checkout's git directory, `None` when none
/// does. A quarantine root that cannot be read as a directory is an error,
/// not a block: the root is never removed, so a block recorded against it
/// would keep every later merge of the stage paused after the operator fixed
/// its permissions.
pub(in crate::git::merge) fn held_block(repo: &Path, stage_id: &str) -> Result<Option<MergeBlock>> {
    let root = git_dir_for_repo_path(repo)?.join(QUARANTINE_ROOT);
    Ok(
        held_quarantine(&root, stage_id)?.map(|quarantine| MergeBlock::CheckoutRestoreIncomplete {
            failures: Vec::new(),
            detail: Some(HELD.into()),
            quarantine,
        }),
    )
}

/// The first entry of `root` named `<stage_id>-<digits>`. An absent `root`
/// holds nothing; one that cannot be read fails closed.
fn held_quarantine(root: &Path, stage_id: &str) -> Result<Option<PathBuf>> {
    let unreadable = |error: io::Error| {
        anyhow!(
            "the quarantine directory {} could not be read as a directory ({error}); \
             fix its permissions or type before the merge can run. It holds other \
             stages' files, so do not delete it",
            root.display()
        )
    };
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unreadable(error)),
    };
    for entry in entries {
        let entry = entry.map_err(unreadable)?;
        if names_attempt(&entry.file_name().to_string_lossy(), stage_id) {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

/// Whether `name` is `<stage_id>-<unix nanos>`, the attempt directory name
/// the quarantine gives a merge of `stage_id`.
fn names_attempt(name: &str, stage_id: &str) -> bool {
    name.strip_prefix(stage_id)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests;
