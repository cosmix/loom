//! The private git directory one pinned command runs with.
//!
//! A session can write its worktree's administrative directory and the common
//! directory, and git opens everything there by path: it reads
//! `$GIT_DIR/config.worktree` with `extensions.worktreeConfig` on, and it writes
//! through a symbolic link at `logs/HEAD`, `COMMIT_EDITMSG`, `MERGE_MSG` or
//! `index`. A link to `~/.bashrc` there would have the daemon append a reflog
//! line naming the session's own commit subject to that file, and any check
//! ahead of git's open is a race. A command here never gets those paths:
//! `GIT_DIR` names a directory only this process writes, inside the state root
//! every session's capsule denies.
//!
//! What the directory holds:
//!
//! - `commondir` names the pinned common directory (git's ref store reads this
//!   file and ignores `GIT_COMMON_DIR`).
//! - `config.worktree` is a validated copy, when the extension is on.
//! - `HEAD`, `index` and the names in [`COPIED_STATE`] are copies, taken without
//!   following a link; a link planted there refuses the command.
//! - `logs` holds an empty `HEAD`, so git's reflog entry for `HEAD` lands here and
//!   is appended to the real file afterwards, in a stage worktree too, where git
//!   creates no new log (`NO_NEW_BRANCH_REFLOG`). A checkout with no such file has
//!   none here either, and the operator's configuration decides. No pinned command
//!   reads a reflog. Entries for other per-worktree refs (`refs/bisect`,
//!   `refs/worktree`) are dropped: no pinned command moves those.
//! - Names in [`LINKED_DIRS`] are symlinks to real directories, refused when
//!   they are links themselves. A directory swapped for a link after that look
//!   is followed; no pinned command writes beneath one, and a command that
//!   creates a state directory (`rebase-merge`, `sequencer`) is reported as an
//!   error, since publishing a tree into a directory an agent can replace is a
//!   write primitive. Revisit if a pinned caller starts a rebase or a bisection.
//!
//! Only exact names are copied, so a file whose name merely folds to
//! `config.worktree` on a case-insensitive filesystem never appears here, and a
//! session gets no copy of a file git would not write. What the names add up to
//! has a budget (`copy_budget`), since a session writes those files.
//!
//! [`ScratchAdmin::publish`] applies what git changed through a descriptor
//! opened at the start, so replacing the directory itself redirects nothing.
//! Each change is a compare and swap under git's own `<name>.lock`: a file that
//! moved since the copy was taken stops the update, and a concurrent git in the
//! worktree cannot change it in between. A `sharedindex.<hash>` file is apart:
//! named after its content, it never changes, so only a new one is published, and
//! ahead of the `index` naming it. One git removed privately stays: the real
//! `index` may still name it, and git's own expiry in the real directory reaps it.
//!
//! The common directory is not covered here: git's ref store writes
//! `<common>/logs/refs/heads/<branch>`, `<common>/refs` and `<common>/packed-refs`
//! whatever `GIT_DIR` names. `deny_reflogs_and_packed_refs` in
//! `sandbox/control_surfaces/session_denies_ref_store.rs` says what closes each.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::os::fd::OwnedFd;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::config_worktree::{snapshot_worktree_config, CONFIG_WORKTREE};
use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{list_dir_no_follow, EntryKind};

#[path = "scratch_admin_files.rs"]
mod files;

#[path = "scratch_admin_publish.rs"]
mod publish;

use files::{copy_budget, copy_in, private_dir, refuse_planted_reflog, Stamp};
pub(super) use files::{open_regular, NoPrivateDirectory};

const HEAD: &str = "HEAD";
const COMMONDIR: &str = "commondir";
const INDEX: &str = "index";
const LOGS: &str = "logs";
const REFS: &str = "refs";
/// Per-worktree files git keeps in its git directory, beside `HEAD`.
const COPIED_STATE: [&str; 22] = [
    "ORIG_HEAD",
    "MERGE_HEAD",
    "MERGE_MODE",
    "MERGE_MSG",
    "MERGE_RR",
    "MERGE_AUTOSTASH",
    "AUTO_MERGE",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "REBASE_HEAD",
    "SQUASH_MSG",
    "COMMIT_EDITMSG",
    "FETCH_HEAD",
    "BISECT_ANCESTORS_OK",
    "BISECT_EXPECTED_REV",
    "BISECT_FIRST_PARENT",
    "BISECT_HEAD",
    "BISECT_LOG",
    "BISECT_NAMES",
    "BISECT_RUN",
    "BISECT_START",
    "BISECT_TERMS",
];

/// What precedes the object id in the name of a split-index file.
const SHARED_INDEX_PREFIX: &str = "sharedindex.";

/// Per-worktree directories. `info` carries `info/sparse-checkout`; git resolves
/// the rest of `info/` in the common directory.
const LINKED_DIRS: [&str; 6] = [
    REFS,
    "info",
    "reftable",
    "sequencer",
    "rebase-merge",
    "rebase-apply",
];

/// One file copied into the private directory.
struct Copied {
    name: OsString,
    real: Stamp,
    private: Stamp,
}

/// One command's private git directory.
pub(super) struct ScratchAdmin {
    dir: TempDir,
    admin_path: PathBuf,
    admin: OwnedFd,
    copied: Vec<Copied>,
    linked: Vec<OsString>,
    index_write_is_optional: bool,
}

impl ScratchAdmin {
    /// Build the directory for the administrative directory `admin` of the
    /// repository at `repo_root`. With `worktree_config` (the extension is on)
    /// a `config.worktree` git would act on refuses the command.
    pub(super) fn prepare(
        repo_root: &Path,
        admin: &Path,
        common: &Path,
        worktree_config: bool,
    ) -> Result<Self> {
        let dir = private_dir(repo_root)?;
        let mut commondir = common.as_os_str().as_encoded_bytes().to_vec();
        commondir.push(b'\n');
        std::fs::write(dir.path().join(COMMONDIR), commondir)
            .context("cannot write the private commondir")?;
        let descriptor =
            safe_open_dirfd(admin).with_context(|| format!("cannot open {}", admin.display()))?;
        if worktree_config {
            snapshot_worktree_config(repo_root, &descriptor, admin, dir.path())?;
        }
        let mut scratch = Self {
            dir,
            admin_path: admin.to_path_buf(),
            admin: descriptor,
            copied: Vec::new(),
            linked: Vec::new(),
            index_write_is_optional: false,
        };
        scratch.copy_state(admin != common)?;
        Ok(scratch)
    }

    /// A lost race over the index drops this command's write instead of
    /// failing it: the command only refreshes the index's cached data.
    pub(super) fn with_optional_index_write(mut self, optional: bool) -> Self {
        self.index_write_is_optional = optional;
        self
    }

    /// The directory `GIT_DIR` names.
    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `linked`: the administrative directory is a linked worktree's.
    fn copy_state(&mut self, linked: bool) -> Result<()> {
        let listing = || format!("cannot list {}", self.admin_path.display());
        let entries = list_dir_no_follow(&self.admin, Path::new("")).with_context(listing)?;
        let has_head_reflog = refuse_planted_reflog(&self.admin, &self.admin_path)?;
        std::fs::create_dir(self.dir.path().join(LOGS)).context("cannot create private logs")?;
        if has_head_reflog || linked {
            std::fs::write(self.dir.path().join(LOGS).join(HEAD), "")
                .context("cannot create the private HEAD reflog")?;
        }
        let mut copied_names = vec![OsString::from(HEAD), OsString::from(INDEX)];
        for (name, kind) in entries {
            if is_copied_state(&name) {
                copied_names.push(name);
            } else if is_linked_dir(&name) {
                self.link_dir(&name, kind)?;
            } else if name == LOGS && kind != EntryKind::Directory {
                bail!(
                    "loom will not run git in the worktree: {} is not a directory",
                    self.admin_path.join(LOGS).display()
                );
            }
        }
        let mut budget = copy_budget(&self.admin, &self.admin_path, &copied_names)?;
        for name in copied_names {
            self.copy(&name, &mut budget)?;
        }
        if !self.linked.iter().any(|name| name == REFS) {
            std::fs::create_dir(self.dir.path().join(REFS))
                .context("cannot create private refs")?;
        }
        if !self.copied.iter().any(|copy| copy.name == HEAD) {
            bail!("cannot read HEAD of {}", self.admin_path.display());
        }
        Ok(())
    }

    fn copy(&mut self, name: &OsStr, budget: &mut u64) -> Result<()> {
        let stamps = copy_in(&self.admin, &self.admin_path, name, self.dir.path(), budget)?;
        if let Some((real, private)) = stamps {
            self.copied.push(Copied {
                name: name.to_os_string(),
                real,
                private,
            });
        }
        Ok(())
    }

    fn link_dir(&mut self, name: &OsStr, kind: EntryKind) -> Result<()> {
        let real = self.admin_path.join(name);
        if kind != EntryKind::Directory {
            bail!(
                "loom will not run git in the worktree: {} is not a directory; git never makes \
                 anything else there",
                real.display()
            );
        }
        symlink(&real, self.dir.path().join(name))
            .with_context(|| format!("cannot link {}", Path::new(name).display()))?;
        self.linked.push(name.to_os_string());
        Ok(())
    }
}

fn is_copied_state(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    COPIED_STATE.contains(&name) || is_shared_index(OsStr::new(name))
}

/// `sharedindex.<object id>`, the one form git writes: a SHA-1 or SHA-256 in
/// lowercase hexadecimal.
fn is_shared_index(name: &OsStr) -> bool {
    let Some(hash) = name
        .to_str()
        .and_then(|name| name.strip_prefix(SHARED_INDEX_PREFIX))
    else {
        return false;
    };
    matches!(hash.len(), 40 | 64)
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_linked_dir(name: &OsStr) -> bool {
    LINKED_DIRS.iter().any(|linked| name == *linked)
}

#[cfg(test)]
#[path = "scratch_admin_tests.rs"]
mod tests;
