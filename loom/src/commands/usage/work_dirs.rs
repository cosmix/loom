//! Which `.loom/work` state directory a `loom usage` run reads its hook
//! ledgers and receipts from.

use std::path::{Path, PathBuf};

/// Resolve the optional hook ledger for the project being reported. An
/// explicit project must use its own state directory; falling back to the
/// caller's repository would silently attach unrelated spawn metadata.
/// `--all` likewise spans repositories and therefore has no single safe
/// ledger.
pub(super) fn usage_work_dir(project: Option<&Path>, all: bool) -> Option<PathBuf> {
    if all {
        return None;
    }
    match project {
        Some(project) => project_work_dir(project).filter(|candidate| candidate.is_dir()),
        None => crate::commands::common::work_dir_path().ok(),
    }
}

pub(super) fn project_work_dir(project: &Path) -> Option<PathBuf> {
    let project = if project.is_absolute() {
        project.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(project)
    };
    Some(
        crate::fs::work_dir::WorkDir::new(project)
            .ok()?
            .root()
            .to_path_buf(),
    )
}
