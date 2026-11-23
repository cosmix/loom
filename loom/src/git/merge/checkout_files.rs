//! Reads and writes the operator's checkout for the off-checkout merge:
//! the status and diff inputs of [`super::checkout_state::classify`], the disk
//! probe behind it, and moving the untracked files a merge replaces aside.

use anyhow::{bail, Context, Result};
use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

use super::checkout_state::{CheckoutProbe, DiskEntry};
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::git::runner::{run_git, run_git_with_env};

mod displace;
mod held;

#[cfg(test)]
pub(super) use displace::inject_before_displace;
pub(super) use displace::{displace_files, Displaced};
pub(super) use held::held_block;

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
        let Ok(rel) = checkout_relative(path) else {
            return DiskEntry::Other;
        };
        if let Some(entry) = parent_entry(self.repo, rel) {
            return entry;
        }
        match std::fs::symlink_metadata(self.repo.join(rel)) {
            Ok(meta) if meta.is_file() => DiskEntry::RegularFile,
            Ok(meta) if meta.is_dir() => DiskEntry::Directory,
            Ok(_) => DiskEntry::Other,
            // A parent that is a file leaves nothing at the path itself.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
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
        let Ok(rel) = checkout_relative(path) else {
            return false;
        };
        open_parent(self.repo, rel)
            .and_then(|parent| Ok(entry_matches(&parent, &entry_name(rel)?, &blob)?))
            .unwrap_or(false)
    }

    fn in_base_tree(&self, path: &str) -> bool {
        tree_entry_mode(self.repo, self.base, path).is_some()
    }
}

/// What a proper prefix of `rel` makes of the path, walking from the top:
/// `None` when every prefix is a directory. A symlinked parent would lead
/// outside the checkout, so it blocks; a file parent leaves the path free.
fn parent_entry(repo: &Path, rel: &Path) -> Option<DiskEntry> {
    let parents: Vec<&Path> = rel
        .ancestors()
        .skip(1)
        .filter(|prefix| !prefix.as_os_str().is_empty())
        .collect();
    for prefix in parents.into_iter().rev() {
        match std::fs::symlink_metadata(repo.join(prefix)) {
            Ok(meta) if meta.is_dir() => continue,
            Ok(meta) if meta.file_type().is_symlink() => return Some(DiskEntry::Other),
            Ok(_) => return Some(DiskEntry::Absent),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Some(DiskEntry::Absent)
            }
            Err(_) => return Some(DiskEntry::Other),
        }
    }
    None
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

/// `path` as a non-empty sequence of plain names: no root, `.` or `..`.
fn checkout_relative(path: &str) -> Result<&Path> {
    let rel = Path::new(path);
    let mut components = rel.components().peekable();
    if components.peek().is_none() || !components.all(|part| matches!(part, Component::Normal(_))) {
        bail!("invalid checkout-relative path");
    }
    Ok(rel)
}

/// The last component of a path accepted by [`checkout_relative`].
fn entry_name(rel: &Path) -> Result<CString> {
    let name = rel.file_name().context("path has no file name")?;
    CString::new(name.as_bytes()).context("path contains a NUL byte")
}

/// Whether `name` under `parent` is a regular file holding exactly `blob`.
/// Never follows a link and reads at most one byte past the blob.
fn entry_matches(parent: &OwnedFd, name: &CStr, blob: &[u8]) -> io::Result<bool> {
    // SAFETY: `parent` is an open directory and `name` is a NUL-terminated
    // basename. O_NOFOLLOW rejects a link; O_NONBLOCK keeps a FIFO from
    // blocking the open.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let error = io::Error::last_os_error();
        // O_NOFOLLOW refuses a symlink with ELOOP: not a regular file.
        if error.raw_os_error() == Some(libc::ELOOP) {
            return Ok(false);
        }
        return Err(error);
    }
    // SAFETY: successful openat returned a fresh fd owned by this function.
    let file = unsafe { File::from_raw_fd(fd) };
    if !file.metadata()?.is_file() {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(blob.len() + 1);
    file.take(blob.len() as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes == blob)
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
