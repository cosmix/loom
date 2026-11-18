//! Operator operations in progress in the main checkout, which make a merge
//! refuse.

use anyhow::{Context, Result};
use std::path::Path;

use super::in_progress::git_dir_for_repo_path;

/// Marker paths relative to the git dir of the main checkout; `sequencer` is
/// a multi-commit cherry-pick or revert stopped between commits.
const OPERATOR_MARKERS: [&str; 6] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "sequencer",
    "rebase-merge",
    "rebase-apply",
];

/// The marker of an operator operation in progress in the main checkout.
/// A git dir that cannot be read is an error, never "no operation".
pub(crate) fn operator_operation(repo_root: &Path) -> Result<Option<String>> {
    let git_dir = git_dir_for_repo_path(repo_root)?;
    for marker in OPERATOR_MARKERS {
        if path_present(&git_dir.join(marker))? {
            return Ok(Some(marker.to_string()));
        }
    }
    Ok(None)
}

/// Whether `path` exists (a symlink is not followed). Only `NotFound` means
/// absent; any other failure is returned.
fn path_present(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Cannot stat {}", path.display())),
    }
}

#[cfg(test)]
mod tests;
