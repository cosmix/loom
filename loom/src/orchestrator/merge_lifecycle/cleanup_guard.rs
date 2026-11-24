//! What keeps a post-merge cleanup from removing a worktree someone still uses
//! or is already removing.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use fs2::FileExt;

/// Serialize cleanups of one stage across processes.
///
/// The daemon's leftover sweep and an operator command such as
/// `loom stage human-review --force-complete` both clean up a stage once it is
/// marked merged. Unserialized, the second `git worktree remove` ran against a
/// tree the first was halfway through deleting, and reported a failure for a
/// cleanup that succeeded. Under this lock the second caller finds nothing left
/// to remove.
///
/// Best effort: a caller that cannot create the lock (a session whose sandbox
/// denies writes to the state directory) cleans up unlocked, as it would have
/// without this guard. The lock is released when the returned file drops.
pub(super) fn lock_stage_cleanup(work_dir: &Path, stage_id: &str) -> Option<File> {
    match try_lock(work_dir, stage_id) {
        Ok(file) => Some(file),
        Err(error) => {
            tracing::warn!(stage = %stage_id, error = %format!("{error:#}"), "Cleaning up without the stage cleanup lock");
            None
        }
    }
}

fn try_lock(work_dir: &Path, stage_id: &str) -> Result<File> {
    crate::validation::validate_id(stage_id)?;
    let directory = work_dir.join("cleanup-locks");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("creating cleanup lock directory {}", directory.display()))?;
    let path = directory.join(format!("{stage_id}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .with_context(|| format!("opening cleanup lock {}", path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("locking {}", path.display()))?;
    Ok(file)
}

/// Whether worktree cleanup for `stage_id` must be deferred rather than run now.
///
/// Removing `repo_root/.worktrees/<stage_id>` while `cwd` is inside it deletes
/// the current process's (and its parent Claude session's) live working
/// directory, which breaks any hooks the session fires afterward — they spawn a
/// shell with a cwd that no longer exists. When that's the case, the caller must
/// skip immediate cleanup and leave it for the orchestrator (which cleans up
/// after killing the session).
pub fn should_defer_cleanup(cwd: &Path, repo_root: &Path, stage_id: &str) -> bool {
    let expected = repo_root.join(".worktrees").join(stage_id);
    let expected = match expected.canonicalize() {
        Ok(p) => p,
        // Worktree doesn't exist on disk - cleanup would be a no-op anyway.
        Err(_) => return false,
    };
    match cwd.canonicalize() {
        Ok(cwd) => cwd.starts_with(&expected),
        // Can't verify cwd is safe - assume the worst and defer.
        Err(_) => true,
    }
}
