//! Trusted, per-session merge target authorization.

use std::path::Path;

use anyhow::{Context, Result};

use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::read_to_string_bounded;
use crate::fs::session_files::validate_session_file_id;

/// Publish a resolver's target before it can launch. Existing records are immutable.
pub(crate) fn publish_merge_snapshot(
    repo_root: &Path,
    work_dir: &Path,
    session_id: &str,
    target: &str,
) -> Result<String> {
    validate_session_file_id(session_id)?;
    let reference = crate::git::branch::branch_ref(target);
    let git = crate::git::worktree::WorktreeGit::pinned_main(repo_root)
        .context("pinning the main checkout for merge target authorization")?;
    let snapshot = git
        .run_checked(&["rev-parse", "--verify", &format!("{reference}^{{commit}}")])
        .context("resolving merge target snapshot")?
        .trim()
        .to_string();
    let dir = work_dir.join("merge-target-snapshots");
    locked_dir_update(&dir, || {
        let path = dir.join(session_id);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => anyhow::bail!("merge target snapshot already exists"),
        }
        atomic_write_locked(&path, &snapshot)
    })?;
    Ok(snapshot)
}

/// Read the immutable authorization for this session, without following links.
pub(crate) fn load_merge_snapshot(work_dir: &Path, session_id: &str) -> Result<String> {
    validate_session_file_id(session_id)?;
    read_to_string_bounded(
        &work_dir.join("merge-target-snapshots"),
        Path::new(session_id),
        128,
    )
    .context("merge target snapshot is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::signing::tests::git_in;

    #[test]
    fn snapshot_is_immutable_and_refuses_symlink_reads() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        git_in(root, &["init", "-b", "main"]);
        git_in(root, &["config", "user.name", "test"]);
        git_in(root, &["config", "user.email", "test@example.com"]);
        git_in(root, &["commit", "--allow-empty", "-m", "first"]);
        let id = "session-test";
        let snapshot = publish_merge_snapshot(root, root, id, "main").unwrap();
        assert!(publish_merge_snapshot(root, root, id, "main").is_err());
        assert_eq!(load_merge_snapshot(root, id).unwrap(), snapshot);
        let path = root.join("merge-target-snapshots").join(id);
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(root.join(".git/HEAD"), &path).unwrap();
        assert!(load_merge_snapshot(root, id).is_err());
    }
}
