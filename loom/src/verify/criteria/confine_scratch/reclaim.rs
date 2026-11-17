//! Creation and removal of command scratches, and reclaiming the orphans.
//!
//! On Linux a scratch is named for its owner,
//! `.loom-command-<pid namespace>-<pid>-<start>-<random>`, the start time in
//! clock ticks since boot from `/proc/<pid>/stat`. An owner killed before it removed its scratch
//! (SIGKILL runs no destructor) leaves it behind; the next process that
//! creates a scratch removes every one whose owner is gone: same uid, same
//! pid namespace, a real directory (never a symlink), a name only this module
//! makes. A pid reused
//! after a reboot with the same start tick would keep an orphan alive; it is
//! then reclaimed once that process exits.
//!
//! A scratch kept for a boundary whose namespace was not proven empty also
//! waits for that namespace to go ([`marker`]). Scratches are created, and
//! swept on a background thread, only under the same roots: outside the
//! working tree and writable by no other user ([`sweepable`]), so every
//! scratch made is one a later sweep can reclaim.
//!
//! Removal first gives the owner access to every directory below the scratch
//! again: a command may leave one 0500 (Go's module cache, the app-server's
//! own read-only directories) and overlayfs leaves its `work/work` 000. That
//! walk follows no symlink ([`removable`]); `std::fs::remove_dir_all` then
//! removes the scratch on descriptors too (`unlinkat`, never following a
//! symlink, since Rust 1.58.1), so it is kept over a removal of our own.
use std::path::PathBuf;

use anyhow::{Context, Result};

/// The name prefix of every command scratch.
pub(super) const PREFIX: &str = ".loom-command-";

#[cfg(target_os = "linux")]
#[path = "marker.rs"]
pub(super) mod marker;
#[cfg(unix)]
#[path = "removable.rs"]
mod removable;

#[cfg(target_os = "linux")]
pub(super) use owner::sweep_once;
#[cfg(unix)]
pub(super) use removable::make_removable;

/// A new scratch under the first usable root of `roots`, named for this
/// process.
pub(super) fn create(roots: impl Iterator<Item = PathBuf>) -> Result<tempfile::TempDir> {
    let prefix = owner::prefix();
    let mut failure = None;
    for root in roots {
        match tempfile::Builder::new().prefix(prefix).tempdir_in(&root) {
            Ok(directory) => return Ok(directory),
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        Some(error) => {
            Err(error).context("create private command scratch outside its working tree")
        }
        None => anyhow::bail!(
            "command scratch needs a root outside its working tree that no other user can write"
        ),
    }
}

/// The roots of `candidates` a scratch may be created under and a sweep may
/// enter: either root-owned and sticky, or this user's own directory no group
/// or other user can write (`mode & 0o022 == 0`, so a 0755 `~/tmp` counts), so
/// no other user can plant or swap an entry. The caller keeps only roots
/// outside the working tree, where a worker could plant one.
#[cfg(target_os = "linux")]
pub(super) fn sweepable(candidates: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    // SAFETY: geteuid has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    candidates
        .filter(|root| {
            std::fs::symlink_metadata(root).is_ok_and(|metadata| {
                let mode = metadata.permissions().mode();
                let sticky = metadata.uid() == 0 && mode & 0o1000 != 0;
                let private = metadata.uid() == uid && mode & 0o022 == 0;
                metadata.is_dir() && (sticky || private)
            })
        })
        .collect()
}

/// Remove `scratch` after restoring owner access below it. Gone already (a
/// concurrent sweep of another process) counts as removed.
#[cfg(target_os = "linux")]
pub(super) fn remove(scratch: &std::path::Path) -> std::io::Result<()> {
    make_removable(scratch);
    match std::fs::remove_dir_all(scratch) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        removed => removed,
    }
}

#[cfg(not(target_os = "linux"))]
mod owner {
    /// Without procfs no owner can be proven gone: scratches stay unowned.
    pub(super) fn prefix() -> &'static str {
        super::PREFIX
    }
}

#[cfg(target_os = "linux")]
mod owner {
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Once, OnceLock};

    use super::{marker, remove, PREFIX};

    /// The process a scratch name records. The pid namespace is part of it: a
    /// pid means nothing in another namespace, where a scratch bound into a
    /// boundary would otherwise look orphaned.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) struct Owner {
        pub(super) namespace: u64,
        pub(super) pid: u32,
        pub(super) start: u64,
    }

    impl Owner {
        /// This process, when procfs shows its namespace and start time.
        pub(super) fn current() -> Option<Self> {
            static CURRENT: OnceLock<Option<Owner>> = OnceLock::new();
            *CURRENT.get_or_init(|| {
                let link = std::fs::read_link("/proc/self/ns/pid").ok()?;
                let link = link.to_str()?.strip_prefix("pid:[")?.strip_suffix(']')?;
                let pid = std::process::id();
                Some(Self {
                    namespace: link.parse().ok()?,
                    pid,
                    start: start_ticks(pid)?,
                })
            })
        }

        /// The owner a scratch name records: `<namespace>-<pid>-<start>-<random>`.
        pub(super) fn of(name: &str) -> Option<Self> {
            let mut parts = name.strip_prefix(PREFIX)?.splitn(4, '-');
            let owner = Self {
                namespace: parts.next()?.parse().ok()?,
                pid: parts.next()?.parse().ok()?,
                start: parts.next()?.parse().ok()?,
            };
            parts.next().filter(|random| !random.is_empty())?;
            Some(owner)
        }

        /// Gone only when this namespace has no such pid, or that pid has
        /// another start time (it was reused); anything unreadable is alive.
        fn gone(&self) -> bool {
            match std::fs::read_to_string(format!("/proc/{}/stat", self.pid)) {
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
                Ok(_) => start_ticks(self.pid).is_some_and(|ticks| ticks != self.start),
            }
        }
    }

    /// `.loom-command-<namespace>-<pid>-<start>-` for this process; the bare
    /// prefix, never reclaimed, when procfs does not show its owner.
    pub(super) fn prefix() -> &'static str {
        static PREFIX_OF_SELF: OnceLock<String> = OnceLock::new();
        PREFIX_OF_SELF.get_or_init(|| match Owner::current() {
            Some(own) => format!("{PREFIX}{}-{}-{}-", own.namespace, own.pid, own.start),
            None => PREFIX.to_string(),
        })
    }

    /// Field 22 of `/proc/<pid>/stat`; the command name before it may hold
    /// spaces and parentheses, so fields are counted after its last `)`.
    fn start_ticks(pid: u32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }

    /// [`sweep`] each of `roots()`, once per process, on a detached thread so
    /// a large orphan delays no command. A process exiting mid-sweep leaves
    /// the rest to the next one.
    pub(in super::super) fn sweep_once(roots: impl FnOnce() -> Vec<PathBuf>) {
        static SWEPT: Once = Once::new();
        SWEPT.call_once(|| {
            let roots = roots();
            let sweeper = std::thread::Builder::new()
                .name("loom-scratch-sweep".into())
                .spawn(move || roots.iter().for_each(|root| sweep(root)));
            if let Err(error) = sweeper {
                tracing::warn!(%error, "cannot start the command scratch sweep");
            }
        });
    }

    /// Remove every orphaned scratch directly under `root` that names an
    /// owner in this pid namespace and is bound into no boundary that may
    /// still have live members.
    pub(super) fn sweep(root: &Path) {
        let Some(own) = Owner::current() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        // SAFETY: geteuid has no preconditions and cannot fail.
        let uid = unsafe { libc::geteuid() };
        for entry in entries.flatten() {
            let Some(owner) = entry.file_name().to_str().and_then(Owner::of) else {
                continue;
            };
            // `DirEntry::metadata` does not follow a symlink.
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let ours = metadata.file_type().is_dir() && metadata.uid() == uid;
            if !ours || owner.namespace != own.namespace || !owner.gone() {
                continue;
            }
            let path = entry.path();
            if marker::may_live(&path) {
                continue;
            }
            if let Err(error) = remove(&path) {
                tracing::warn!(path = %path.display(), %error, "cannot reclaim orphaned scratch");
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "reclaim_tests.rs"]
mod tests;
