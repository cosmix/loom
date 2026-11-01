//! Reads and writes the operator's checkout for the off-checkout merge:
//! the status and diff inputs of [`super::checkout_state::classify`], the disk
//! probe behind it, and removing and restoring the untracked files a merge
//! replaces.

use anyhow::{bail, Context, Result};
use std::ffi::{CString, OsStr};
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path};

use super::checkout_state::{CheckoutProbe, DiskEntry};
use super::tree::{MergeBlock, RestoreFailure};
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::git::runner::{run_git, run_git_with_env};

/// Stdout of a git command that must succeed, untrimmed (`-z` output).
pub(super) fn checked_stdout(
    args: &[&str],
    env: &[(&str, &OsStr)],
    repo: &Path,
) -> Result<Vec<u8>> {
    let output = run_git_with_env(args, env, repo)?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// `git status --porcelain=v2 -z --untracked-files=all` without taking
/// `.git/index.lock`, which would block the operator's own git.
pub(super) fn porcelain_status(repo: &Path) -> Result<Vec<u8>> {
    checked_stdout(
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        &[("GIT_OPTIONAL_LOCKS", OsStr::new("0"))],
        repo,
    )
}

/// Reads the checkout at `repo` against the merge `base` -> `tree`.
pub(super) struct DiskProbe<'a> {
    pub repo: &'a Path,
    pub base: &'a str,
    pub tree: &'a str,
}

impl CheckoutProbe for DiskProbe<'_> {
    fn disk(&self, path: &str) -> DiskEntry {
        match std::fs::symlink_metadata(self.repo.join(path)) {
            Ok(meta) if meta.is_file() => DiskEntry::RegularFile,
            Ok(meta) if meta.is_dir() => DiskEntry::Directory,
            Ok(_) => DiskEntry::Other,
            // A parent that is a file leaves nothing at the path itself.
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                DiskEntry::Absent
            }
            // An unreadable path cannot be proven free: treat it as occupied.
            Err(_) => DiskEntry::Other,
        }
    }

    fn equals_blob(&self, path: &str) -> bool {
        let is_regular = tree_entry_mode(self.repo, self.tree, path)
            .is_some_and(|mode| mode == "100644" || mode == "100755");
        if !is_regular || self.disk(path) != DiskEntry::RegularFile {
            return false;
        }
        let Some(blob) = read_blob(self.repo, self.tree, path) else {
            return false;
        };
        std::fs::read(self.repo.join(path)).is_ok_and(|bytes| bytes == blob)
    }

    fn in_base_tree(&self, path: &str) -> bool {
        tree_entry_mode(self.repo, self.base, path).is_some()
    }
}

/// `ls-tree -z` prints `<mode> <type> <oid>\t<path>`; the mode of the entry
/// at `path` in `rev`, if it exists.
fn tree_entry_mode(repo: &Path, rev: &str, path: &str) -> Option<String> {
    let output = run_git(&["ls-tree", "-z", rev, "--", path], repo).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (mode, _) = text.split_once(' ')?;
    (!mode.is_empty()).then(|| mode.to_string())
}

fn read_blob(repo: &Path, rev: &str, path: &str) -> Option<Vec<u8>> {
    let blob = run_git(&["cat-file", "blob", &format!("{rev}:{path}")], repo).ok()?;
    blob.status.success().then_some(blob.stdout)
}

/// Failure to remove a checkout file, including any rollback failures.
#[derive(Debug)]
pub(super) struct RemoveFailure {
    pub path: String,
    pub error: io::Error,
    pub restore: RestoreReport,
}

impl fmt::Display for RemoveFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot remove {}: {}", self.path, self.error)?;
        if !self.restore.failures.is_empty() {
            write!(f, "; rollback incomplete: {:?}", self.restore.failures)?;
        }
        Ok(())
    }
}

/// Every failed path is retained so callers can report incomplete recovery.
#[derive(Debug, Default)]
pub(super) struct RestoreReport {
    pub failures: Vec<RestoreFailure>,
}

impl RestoreReport {
    pub fn block(self, rev: &str, backup_ref: Option<String>) -> MergeBlock {
        MergeBlock::CheckoutRestoreIncomplete {
            source_rev: rev.to_string(),
            failures: self.failures,
            backup_ref,
        }
    }
}

/// Remove `paths`, whose bytes equal their blobs in `rev`. On failure,
/// restore earlier removals and retain any recovery failures in the result.
pub(super) fn remove_files(
    repo: &Path,
    rev: &str,
    paths: &[String],
) -> std::result::Result<(), RemoveFailure> {
    for (index, path) in paths.iter().enumerate() {
        if let Err(error) = std::fs::remove_file(repo.join(path)) {
            #[cfg(test)]
            tests::before_restore(repo);
            return Err(RemoveFailure {
                path: path.clone(),
                error,
                restore: restore_files(repo, rev, &paths[..index]),
            });
        }
    }
    Ok(())
}

/// Restore removed untracked files without replacing any occupied pathname.
pub(super) fn restore_files(repo: &Path, rev: &str, paths: &[String]) -> RestoreReport {
    let mut report = RestoreReport::default();
    for path in paths {
        if let Err(error) = restore_one(repo, rev, path) {
            report.failures.push(RestoreFailure {
                path: path.clone(),
                reason: format!("{error:#}"),
            });
        }
    }
    report
}

fn restore_one(repo: &Path, rev: &str, path: &str) -> Result<()> {
    let mode = tree_entry_mode(repo, rev, path)
        .filter(|mode| mode == "100644" || mode == "100755")
        .with_context(|| format!("regular blob mode unavailable at {rev}:{path}"))?;
    let blob =
        read_blob(repo, rev, path).with_context(|| format!("blob unavailable at {rev}:{path}"))?;
    let rel = Path::new(path);
    if !rel
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("invalid checkout-relative path");
    }
    let parent = open_parent(repo, rel)?;
    let name = CString::new(rel.file_name().unwrap().as_bytes())?;
    // SAFETY: `parent` is an open directory and `name` is a NUL-terminated
    // basename. O_EXCL reserves only a missing entry; O_NOFOLLOW rejects links.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o666,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error()).context("destination is occupied or unavailable");
    }
    // SAFETY: successful openat returned a fresh fd owned by this function.
    let mut file = unsafe { File::from_raw_fd(fd) };
    if mode == "100755" {
        file.set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(test)]
    if tests::fail_restored_write() {
        bail!("simulated restored-file write failure");
    }
    file.write_all(&blob)
        .context("cannot write restored blob")?;
    file.sync_all().context("cannot sync restored blob")?;
    Ok(())
}

fn open_parent(repo: &Path, rel: &Path) -> Result<OwnedFd> {
    let root = safe_open_dirfd(repo)?;
    let parent = rel.parent().unwrap_or(Path::new(""));
    if parent.as_os_str().is_empty() {
        return Ok(root);
    }
    open_safely(
        root.as_raw_fd(),
        parent,
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )
    .with_context(|| {
        format!(
            "cannot open parent {} without following symlinks",
            parent.display()
        )
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) fn inject_before_restore(hook: impl FnOnce(&Path) + 'static) {
    tests::inject_before_restore(Box::new(hook));
}
