//! Frozen files whose worktree content no longer matches the freeze record.
//!
//! The completion check fails on drift, and contract-dispute admission
//! refuses a filing while any drift stands: a frozen file changes only
//! through a dispute's replacement, never in the worktree first.

use anyhow::{Context, Result};
use std::path::Path;

use super::store::{self, FreezeRecord};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::utils::sha256_hex;

/// One frozen file that differs from the freeze. `path` is the record's form,
/// relative to the acceptance directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub path: String,
    pub kind: DriftKind,
}

/// How a frozen file differs from the freeze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftKind {
    /// Its bytes hash to something other than the recorded sha256.
    Changed,
    /// It is gone from the worktree.
    Missing,
    /// It could not be read within the frozen-file bound or safely (a symlink,
    /// a path escaping the worktree); the text says why.
    Unreadable(String),
}

/// The drift of every file `record` froze, read under `acceptance_dir`, which
/// must lie inside `worktree_root`. Only the record's files count: a new file
/// matching a harness glob is not drift.
pub fn frozen_drift(
    record: &FreezeRecord,
    acceptance_dir: &Path,
    worktree_root: &Path,
) -> Result<Vec<Drift>> {
    let root = worktree_root.canonicalize()?;
    let working = acceptance_dir.canonicalize()?;
    let inside = working
        .strip_prefix(&root)
        .context("the acceptance directory is outside the worktree")?;
    let drift = record.files.iter().filter_map(|file| {
        let kind = match read_bounded(
            &root,
            &inside.join(&file.path),
            store::MAX_FROZEN_FILE_BYTES,
        ) {
            Ok(bytes) if sha256_hex(&bytes) == file.sha256 => return None,
            Ok(_) => DriftKind::Changed,
            Err(error) if is_not_found(&error) => DriftKind::Missing,
            Err(error) => DriftKind::Unreadable(format!("{error:#}")),
        };
        Some(Drift {
            path: file.path.clone(),
            kind,
        })
    });
    Ok(drift.collect())
}
