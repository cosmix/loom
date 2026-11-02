//! The capsule's denies on the common directory's ref store, and what keeps a
//! session's ref updates working under them.

use anyhow::{bail, Context, Result};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::SessionDenies;
use crate::git::branch::branch_name_for_stage;

/// Deny the reflogs and `packed-refs` of the common directory. Git opens a ref's
/// reflog by path and appends through a link there, and that path is the common
/// directory's whatever `GIT_DIR` names, so a session able to plant
/// `logs/refs/heads/<branch>` aims the daemon's next ref update at any file
/// (`core.logAllRefUpdates=false` does not stop an append to a file that exists).
/// `packed-refs` as a link has git replace its target with a list of refs the
/// session named.
///
/// A session's own ref updates have to finish under this deny, or stop before
/// they touch anything. Left alone they do neither: `git reset <commit>`, a bare
/// `git merge` and `git cherry-pick` move the index and work tree first, then the
/// refused reflog stops the ref, and a later `git add` plus `loom stage commit`
/// commits a tree reversing the stage's work (measured). With
/// `core.logAllRefUpdates=false` and no log file git writes no reflog and asks
/// nothing of `logs/`, so three things hold that state for a stage branch:
///
/// - each spawn removes the log ([`remove_stage_branch_reflog`]), whoever wrote it;
/// - a pinned command in a stage worktree runs with the setting (`WorktreeGit`);
/// - the session wrapper exports it (`WorktreeGit::SESSION_GIT_PARAMETERS`).
///
/// So on its stage branch a session's `git reset`, `git merge`, `git commit`,
/// `git rebase` and `git branch` all finish, and a stage branch has no reflog:
/// `HEAD@{n}` still works, from the worktree's own `logs/HEAD` in the
/// administrative directory, and `@{n}` refuses. `git stash` forces a reflog, so it
/// refuses before it cleans anything.
///
/// Still half applied, with the loud error above and `git reset --hard HEAD` the
/// way back: a command run without the setting (a session overriding it, or an
/// `env -i`), and a session rooted in the checkout, whose branch keeps the
/// operator's reflog. Revisit if git gains an option to skip a reflog it cannot
/// write.
///
/// `refs/` stays writable: git takes `refs/heads/<branch>.lock` even for a move
/// to the same commit, so denying it fails `git merge --abort` and `git reset` on
/// a branch, the merge session's own way out (measured). A directory link under
/// `refs/heads` therefore stays plantable, and has the daemon's ref update write
/// the 41 bytes of an object id to `<link target>/<ref name>`. The runner looks
/// for one ahead of each ref update (`refuse_ref_path_links`); a writer that
/// flips the link between that look and git's open is not stopped. Revisit if git
/// gains a way to skip the lock on a move to the same commit.
///
/// A directory deny is complete: a bind of a directory outlives what the daemon
/// does inside it. A deny of one file is not: the daemon replacing the file by
/// rename detaches the read-only bind from the session's mount namespace, and
/// the session can then replace it (measured). `packed-refs` is therefore also
/// looked at ahead of each command, with the same race, and only after a rewrite
/// of the file during the session. Loom's own commands run no automatic
/// maintenance, so what rewrites it is a deletion of a packed ref or the
/// operator's own git.
///
/// A deny on an absent path shows the daemon an empty placeholder, so the paths
/// are created first. A link there would bind its target, so it refuses the spawn.
pub(super) fn deny_reflogs_and_packed_refs(
    denies: &mut SessionDenies,
    common: &Path,
    worktree: Option<&Path>,
) -> Result<()> {
    if !common.is_dir() {
        return Ok(());
    }
    let logs = common.join("logs");
    match std::fs::create_dir(&logs) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("cannot create {}", logs.display()))
        }
    }
    require_not_a_link(&logs)?;
    if let Some(worktree) = worktree {
        remove_stage_branch_reflog(&logs, worktree)?;
    }
    denies.absolute(&logs, true)?;
    let packed = common.join("packed-refs");
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&packed)
        .with_context(|| format!("cannot create {}", packed.display()))?;
    denies.absolute(&packed, false)
}

/// Remove the reflog of the branch a stage worktree is named for, the one branch
/// its session moves. The name comes from the worktree's directory, never its
/// `HEAD`, which the session writes: a `HEAD` on `main` must not cost the operator
/// a reflog. A session cannot write under `logs`, so no link is on the way, and a
/// link at the leaf is removed unfollowed.
fn remove_stage_branch_reflog(logs: &Path, worktree: &Path) -> Result<()> {
    let Some(stage) = worktree.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let log = logs.join("refs/heads").join(branch_name_for_stage(stage));
    match std::fs::remove_file(&log) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("cannot remove {}", log.display())),
    }
}

fn require_not_a_link(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect {}", path.display()))?;
    if meta.is_symlink() {
        bail!(
            "cannot confine a session: {} is a symbolic link",
            path.display()
        );
    }
    Ok(())
}
