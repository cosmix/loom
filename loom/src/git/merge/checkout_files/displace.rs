//! Moves the untracked files a merge replaces into a private quarantine under
//! the checkout's git directory, and puts them back or discards them once the
//! fast-forward has failed or landed.
//!
//! The comparison that decides whether a file may be replaced is made on the
//! moved entry, through the quarantine's descriptor: bytes the operator wrote
//! at any earlier moment are never deleted, and the way back uses `linkat`,
//! which never replaces an entry now at the path.

use anyhow::{bail, Context, Result};
use std::ffi::{CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{checkout_relative, entry_matches, entry_name, open_parent, read_blob};
use crate::fs::no_follow_tree::remove_entry;
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::git::merge::in_progress::git_dir_for_repo_path;
use crate::git::merge::tree::{MergeBlock, RestoreFailure};

/// Directory under the git dir that holds one attempt directory per merge.
pub(super) const QUARANTINE_ROOT: &str = "loom-displaced";
const CHANGED: &str = "changed while the merge moved it aside";

/// Untracked files moved out of the checkout, waiting for the merge outcome.
#[must_use]
#[derive(Debug, Default)]
pub(in crate::git::merge) struct Displaced {
    repo: PathBuf,
    quarantine: PathBuf,
    attempt: Option<OwnedFd>,
    base: Option<OwnedFd>,
    name: OsString,
    moved: Vec<String>,
}

/// Move `paths`, untracked files whose bytes should equal their blobs in
/// `rev`, out of the checkout at `repo`. Every moved entry is compared after
/// the move; any doubt blocks the merge and keeps the bytes.
pub(in crate::git::merge) fn displace_files(
    repo: &Path,
    stage_id: &str,
    rev: &str,
    paths: &[String],
) -> Result<Displaced, MergeBlock> {
    if paths.is_empty() {
        return Ok(Displaced::default());
    }
    let overlap = || MergeBlock::UncommittedOverlap {
        paths: paths.to_vec(),
    };
    if paths.iter().any(|path| checkout_relative(path).is_err()) {
        return Err(overlap());
    }
    let mut displaced = match Displaced::open(repo, stage_id) {
        Ok(displaced) => displaced,
        Err(error) => {
            tracing::warn!(repo = %repo.display(), "cannot open the merge quarantine: {error:#}");
            return Err(overlap());
        }
    };
    #[cfg(test)]
    run_before_displace(repo);
    let (stopped, kept) = displaced.move_all(rev, paths);
    if !stopped && kept.is_empty() {
        return Ok(displaced);
    }
    Err(displaced.unwind(kept).unwrap_or_else(overlap))
}

impl Displaced {
    /// Create `<git dir>/loom-displaced/<stage_id>-<unix nanos>/`, refusing a
    /// link at either level and an attempt directory that already exists.
    fn open(repo: &Path, stage_id: &str) -> Result<Self> {
        if stage_id.is_empty() || stage_id.contains('/') {
            bail!("stage id {stage_id:?} cannot name a quarantine directory");
        }
        let git_dir = git_dir_for_repo_path(repo)?;
        let git = safe_open_dirfd(&git_dir)?;
        let root = OsStr::new(QUARANTINE_ROOT);
        make_dir(&git, root, true)?;
        let base = open_dir(&git, root)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = OsString::from(format!("{stage_id}-{nanos}"));
        make_dir(&base, &name, false)?;
        let attempt = open_dir(&base, &name)?;
        Ok(Self {
            repo: repo.to_path_buf(),
            quarantine: git_dir.join(QUARANTINE_ROOT).join(&name),
            attempt: Some(attempt),
            base: Some(base),
            name,
            moved: Vec::new(),
        })
    }

    /// Move each path in order and compare the moved entry with its blob.
    /// Returns whether a move failed and stopped the loop, and the paths kept.
    fn move_all(&mut self, rev: &str, paths: &[String]) -> (bool, Vec<RestoreFailure>) {
        let Some(attempt) = &self.attempt else {
            return (true, Vec::new());
        };
        let mut kept = Vec::new();
        for path in paths {
            if let Err(error) = move_one(&self.repo, attempt, path) {
                tracing::warn!(path, "cannot move an untracked file aside: {error:#}");
                return (true, kept);
            }
            self.moved.push(path.clone());
            if let Some(reason) = verify_moved(&self.repo, rev, attempt, path) {
                kept.push(RestoreFailure {
                    path: path.clone(),
                    reason,
                });
            }
        }
        (false, kept)
    }

    /// Put back every moved path that is not `kept`; `None` when nothing is
    /// kept and every put-back worked.
    fn unwind(self, mut kept: Vec<RestoreFailure>) -> Option<MergeBlock> {
        let put_back = self.put_back_except(&kept);
        kept.extend(put_back);
        self.finish(kept)
    }

    /// The fast-forward landed and the checkout holds the same bytes: remove
    /// the quarantined copies.
    pub(in crate::git::merge) fn discard(self) {
        self.remove_attempt();
    }

    /// The merge did not land: put every moved file back without replacing
    /// anything now at its path. `Some` names what stays in quarantine.
    pub(in crate::git::merge) fn restore(self) -> Option<MergeBlock> {
        let failures = self.put_back_except(&[]);
        self.finish(failures)
    }

    fn put_back_except(&self, kept: &[RestoreFailure]) -> Vec<RestoreFailure> {
        let Some(attempt) = &self.attempt else {
            return Vec::new();
        };
        self.moved
            .iter()
            .rev()
            .filter(|path| !kept.iter().any(|failure| &failure.path == *path))
            .filter_map(|path| {
                put_back(&self.repo, attempt, path)
                    .err()
                    .map(|error| RestoreFailure {
                        path: path.clone(),
                        reason: format!("could not be put back: {error:#}"),
                    })
            })
            .collect()
    }

    fn finish(self, failures: Vec<RestoreFailure>) -> Option<MergeBlock> {
        if failures.is_empty() {
            self.remove_attempt();
            return None;
        }
        Some(MergeBlock::CheckoutRestoreIncomplete {
            quarantine: self.quarantine,
            failures,
            detail: None,
        })
    }

    fn remove_attempt(&self) {
        let Some(base) = &self.base else {
            return;
        };
        if let Err(error) = remove_entry(base.as_fd(), &self.name, 0) {
            tracing::warn!(
                quarantine = %self.quarantine.display(),
                "cannot remove the merge quarantine: {error}"
            );
        }
    }
}

/// `renameat` from the checkout into a fresh name in a directory loom just
/// made, so nothing is replaced. Both sides must share a filesystem; `EXDEV`
/// moves nothing and blocks the merge.
fn move_one(repo: &Path, attempt: &OwnedFd, path: &str) -> Result<()> {
    let rel = checkout_relative(path)?;
    let name = entry_name(rel)?;
    let from = open_parent(repo, rel)?;
    let to = quarantine_parent(attempt, rel, true)?;
    // SAFETY: both descriptors are open directories and `name` is a
    // NUL-terminated basename.
    let moved = unsafe {
        libc::renameat(
            from.as_raw_fd(),
            name.as_ptr(),
            to.as_raw_fd(),
            name.as_ptr(),
        )
    };
    if moved != 0 {
        return Err(io::Error::last_os_error()).context("cannot move the file into quarantine");
    }
    Ok(())
}

/// The reason to keep the moved `path` in quarantine, if any.
fn verify_moved(repo: &Path, rev: &str, attempt: &OwnedFd, path: &str) -> Option<String> {
    let compared = (|| -> Result<bool> {
        let blob = read_blob(repo, rev, path)
            .with_context(|| format!("blob unavailable at {rev}:{path}"))?;
        let rel = checkout_relative(path)?;
        let parent = quarantine_parent(attempt, rel, false)?;
        Ok(entry_matches(&parent, &entry_name(rel)?, &blob)?)
    })();
    match compared {
        Ok(true) => None,
        Ok(false) => Some(CHANGED.to_string()),
        Err(error) => Some(format!(
            "could not be compared with the merge's blob: {error:#}"
        )),
    }
}

/// Link the quarantined entry back at its checkout path, then drop the
/// quarantine name. `linkat` fails with `EEXIST` on an occupied path.
fn put_back(repo: &Path, attempt: &OwnedFd, path: &str) -> Result<()> {
    let rel = checkout_relative(path)?;
    let name = entry_name(rel)?;
    let from = quarantine_parent(attempt, rel, false)?;
    let to = open_parent(repo, rel)?;
    // SAFETY: both descriptors are open directories and `name` is a
    // NUL-terminated basename; flags 0 links the entry itself.
    let linked = unsafe {
        libc::linkat(
            from.as_raw_fd(),
            name.as_ptr(),
            to.as_raw_fd(),
            name.as_ptr(),
            0,
        )
    };
    if linked != 0 {
        return Err(io::Error::last_os_error()).context("the path is occupied or unavailable");
    }
    // SAFETY: `from` is an open directory and `name` a NUL-terminated basename.
    if unsafe { libc::unlinkat(from.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(io::Error::last_os_error())
            .context("the file is back, but its quarantine copy remains");
    }
    Ok(())
}

/// The directory inside the attempt that mirrors the parent of `rel`,
/// created component by component when `create` is set.
fn quarantine_parent(attempt: &OwnedFd, rel: &Path, create: bool) -> Result<OwnedFd> {
    let mut dir = attempt.try_clone()?;
    for part in rel.parent().into_iter().flat_map(Path::components) {
        let part = part.as_os_str();
        if create {
            make_dir(&dir, part, true)?;
        }
        dir = open_dir(&dir, part)?;
    }
    Ok(dir)
}

/// `mkdirat` mode 0700; an existing entry is fine only when `exist_ok`.
fn make_dir(parent: &OwnedFd, name: &OsStr, exist_ok: bool) -> Result<()> {
    let c_name = CString::new(name.as_bytes())?;
    // SAFETY: `parent` is an open directory and `c_name` a NUL-terminated basename.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), c_name.as_ptr(), 0o700) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if exist_ok && error.kind() == io::ErrorKind::AlreadyExists {
        return Ok(());
    }
    Err(error).with_context(|| format!("cannot create {}", Path::new(name).display()))
}

/// Open the directory `name` under `parent`, refusing a link.
fn open_dir(parent: &OwnedFd, name: &OsStr) -> Result<OwnedFd> {
    open_safely(
        parent.as_raw_fd(),
        Path::new(name),
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )
    .with_context(|| {
        format!(
            "cannot open {} without following links",
            Path::new(name).display()
        )
    })
}

#[cfg(test)]
type BeforeDisplace = Box<dyn FnOnce(&Path)>;

#[cfg(test)]
thread_local! {
    static BEFORE_DISPLACE: std::cell::RefCell<Option<BeforeDisplace>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `hook` with the checkout root once the quarantine is open and before
/// the first file moves.
#[cfg(test)]
pub(in crate::git::merge) fn inject_before_displace(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_DISPLACE.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_before_displace(repo: &Path) {
    let hook = BEFORE_DISPLACE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(repo);
    }
}

#[cfg(test)]
mod tests;
