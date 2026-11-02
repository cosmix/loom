//! Stash the main checkout's tracked changes from plumbing commands.
//!
//! `git stash create` and `git stash push` compare the work tree with gitlink
//! recursion on and have no `--ignore-submodules`: for a gitlink whose directory
//! holds a `.git`, they start `git status` there, which runs what that
//! repository's configuration names (its clean filters, for one). A
//! `diff.ignoreSubmodules` setting does not reach them, and
//! `submodule.<name>.ignore` applies only to a path the checkout's own
//! `.gitmodules` lists. So this builds the commit shape `git stash` writes from
//! commands the runner keeps out of gitlinks, the way `status` and `diff` run.
//! A gitlink's moved commit stays out of the stash and in place, as with
//! `git stash`, which leaves out every path that is a directory on disk. Files
//! modified inside a submodule stay where they are, which `git stash` could not
//! save either. `checkout_stash/tests.rs` compares the commit against the one
//! `git stash create` writes.
//!
//! `stash store`, `stash apply` and `reset --hard --no-recurse-submodules` start
//! nothing in a gitlink, so they stay the git commands they are.

use anyhow::{bail, ensure, Context, Result};
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::checkout_files::checked_stdout;
use super::tree::rev_parse;
use crate::git::runner::{run_git_checked, run_git_with_stdin};

/// A commit of the checkout's tracked changes, in the shape `git stash` writes:
/// its tree is the work tree, its parents are `HEAD` and a commit of the index.
pub(super) struct ChangesSnapshot {
    pub commit: String,
    message: String,
    trees: CheckoutTrees,
}

#[derive(PartialEq, Eq)]
struct CheckoutTrees {
    index: String,
    work: String,
}

/// Commit the tracked changes without touching the checkout; `None` when both
/// the index and the work tree match `HEAD`. Fails on what `git stash create`
/// also refuses: an unmerged index or an intent-to-add entry.
pub(super) fn snapshot_changes(repo: &Path) -> Result<Option<ChangesSnapshot>> {
    let head = rev_parse(repo, "HEAD")?;
    let trees = checkout_trees(repo, &head)?;
    let head_tree = run_git_checked(
        &["rev-parse", "--verify", &format!("{head}^{{tree}}")],
        repo,
    )?;
    if trees.index == head_tree && trees.work == head_tree {
        return Ok(None);
    }
    let branch = run_git_checked(&["symbolic-ref", "--short", "HEAD"], repo)?;
    let subject = run_git_checked(&["log", "-1", "--format=%h %s", &head], repo)?;
    let index_message = format!("index on {branch}: {subject}");
    let index_commit = run_git_checked(
        &[
            "commit-tree",
            &trees.index,
            "-p",
            &head,
            "-m",
            &index_message,
        ],
        repo,
    )?;
    let message = format!("WIP on {branch}: {subject}");
    let commit = run_git_checked(
        &[
            "commit-tree",
            &trees.work,
            "-p",
            &head,
            "-p",
            &index_commit,
            "-m",
            &message,
        ],
        repo,
    )?;
    Ok(Some(ChangesSnapshot {
        commit,
        message,
        trees,
    }))
}

/// The index and work trees, read from one private copy of the real index, so
/// the real index is neither locked nor written. As in `git stash`, the copy keeps
/// its skip-worktree bits (an out-of-cone path absent from disk is not a deletion)
/// and only tracked paths that differ from `HEAD` are read from disk.
fn checkout_trees(repo: &Path, head: &str) -> Result<CheckoutTrees> {
    let scratch = tempfile::tempdir().context("Cannot create a private index directory")?;
    let private_index = scratch.path().join("index");
    copy_index(repo, &private_index)?;
    let env = [("GIT_INDEX_FILE", private_index.as_os_str())];
    let intent_to_add = checked_stdout(
        &["diff-files", "--diff-filter=A", "--name-only", "-z"],
        &env,
        repo,
    )?;
    if !intent_to_add.is_empty() {
        bail!(
            "intent-to-add entries cannot be stashed: {}",
            String::from_utf8_lossy(&intent_to_add).replace('\0', " ")
        );
    }
    let index = write_tree(repo, &env)?;
    let changed = checked_stdout(&["diff-index", "--name-only", "-z", head], &env, repo)?;
    update_private_index(
        repo,
        scratch.path(),
        &without_directories(repo, &changed),
        &env,
    )?;
    Ok(CheckoutTrees {
        index,
        work: write_tree(repo, &env)?,
    })
}

fn write_tree(repo: &Path, env: &[(&str, &OsStr)]) -> Result<String> {
    let tree = checked_stdout(&["write-tree"], env, repo)?;
    Ok(String::from_utf8_lossy(&tree).trim().to_string())
}

/// Copy the real index to `private`, keeping its modification time: git compares
/// an entry's content when its file changed in the second the index was written,
/// which it learns only from that time. A later time would read such an edit as
/// clean and the reset would lose it. A missing index stays missing, which git
/// reads as an empty one.
fn copy_index(repo: &Path, private: &Path) -> Result<()> {
    let index = repo.join(run_git_checked(
        &["rev-parse", "--git-path", "index"],
        repo,
    )?);
    let mut source = match File::open(&index) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("Cannot read {}", index.display()))
        }
    };
    let modified = source.metadata()?.modified()?;
    let mut copy = File::create(private).context("Cannot create the private index")?;
    io::copy(&mut source, &mut copy).context("Cannot copy the index")?;
    copy.set_modified(modified)
        .context("Cannot keep the index's modification time")
}

/// `changed` without the paths that are directories on disk, which `git stash`
/// also leaves out of its work tree: a gitlink, or a tracked file a directory
/// replaced.
fn without_directories(repo: &Path, changed: &[u8]) -> Vec<u8> {
    let mut kept = Vec::with_capacity(changed.len());
    for path in changed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let on_disk = repo.join(OsStr::from_bytes(path));
        if !std::fs::symlink_metadata(on_disk).is_ok_and(|metadata| metadata.is_dir()) {
            kept.extend_from_slice(path);
            kept.push(0);
        }
    }
    kept
}

/// Read the NUL-separated `changed` paths from disk into the private index. They
/// travel on stdin because a path need not be UTF-8 and the list has no bound.
fn update_private_index(
    repo: &Path,
    scratch: &Path,
    changed: &[u8],
    env: &[(&str, &OsStr)],
) -> Result<()> {
    if changed.is_empty() {
        return Ok(());
    }
    let listing = scratch.join("changed-paths");
    std::fs::write(&listing, changed).context("Cannot write the changed-path listing")?;
    let stdin = File::open(&listing).context("Cannot reopen the changed-path listing")?;
    let args = [
        "update-index",
        "--ignore-skip-worktree-entries",
        "-z",
        "--add",
        "--remove",
        "--stdin",
    ];
    let output = run_git_with_stdin(&args, env, stdin, repo)?;
    if !output.status.success() {
        bail!(
            "git update-index failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Add `snapshot` as the top stash entry, unless the checkout changed after it
/// was taken: `reset_checkout` would then discard a change no copy holds. This
/// narrows that window and does not close it. From this comparison to the end of
/// the reset lie several process spawns, hashing through clean filters and the
/// reset's own walk of the tree, which can take seconds on a large checkout; an
/// edit made in that time is lost, as it is with `git stash push`.
pub(super) fn store_unchanged(repo: &Path, snapshot: &ChangesSnapshot) -> Result<()> {
    // Takes the real index's lock, as `git stash` does before it writes anything: a
    // held lock would fail the reset once the entry exists, and that needs the
    // operator, where failing here is retried.
    run_git_checked(&["write-tree"], repo)?;
    let current = checkout_trees(repo, &rev_parse(repo, "HEAD")?)?;
    ensure!(
        current == snapshot.trees,
        "the checkout changed while the merge was being prepared"
    );
    let args = [
        "stash",
        "store",
        "--quiet",
        "-m",
        &snapshot.message,
        &snapshot.commit,
    ];
    run_git_checked(&args, repo).map(|_| ())
}

/// Drop the stashed changes from the checkout, as `git stash push` does.
pub(super) fn reset_checkout(repo: &Path) -> Result<()> {
    // Unpinned, as the fast-forward is (see `try_fast_forward`): a held index lock
    // must stop this before it changes the checkout.
    let args = ["reset", "--hard", "--quiet", "--no-recurse-submodules"];
    run_git_checked(&args, repo).map(|_| ())
}

/// Whether the checkout still holds exactly what `snapshot` recorded. `false`
/// when that cannot be established.
pub(super) fn checkout_unchanged(repo: &Path, snapshot: &ChangesSnapshot) -> bool {
    rev_parse(repo, "HEAD")
        .and_then(|head| checkout_trees(repo, &head))
        .is_ok_and(|current| current == snapshot.trees)
}

#[cfg(test)]
mod tests;
