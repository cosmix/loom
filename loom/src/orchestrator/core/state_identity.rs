//! Detects a state directory that was removed and recreated out from under a
//! running daemon (e.g. `loom clean --all` followed by `loom init` for a
//! different plan). The daemon's singleton flock stays held on the old,
//! unlinked `orchestrator.lock` inode, so nothing about the flock itself
//! notices the swap; comparing device/inode identity on every tick does.
//!
//! Mirrors the inode-rotation check `daemon::server::broadcast` already uses
//! for log tailing (`fs::metadata(path).ino()` vs. the held value).

use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Name of the daemon's singleton lock file. Mirrors the `LOCK_FILE` constant
/// in `daemon::server::lock`, which is private to that module tree and out of
/// reach from here; kept as a literal rather than widening cross-module
/// visibility for a single filename.
const LOCK_FILE: &str = "orchestrator.lock";

/// Device and inode of the singleton lock file the daemon holds open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockIdentity {
    pub dev: u64,
    pub ino: u64,
}

impl LockIdentity {
    /// Identity of an already-open file, via `fstat`.
    pub fn of_file(file: &std::fs::File) -> std::io::Result<Self> {
        let metadata = file.metadata()?;
        Ok(Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }

    /// `of_file`, but state-identity checks are best-effort: a failed
    /// `fstat` disables them for this run instead of aborting daemon
    /// startup over it, since they exist to catch a rare state-directory
    /// swap, not to gate normal operation.
    pub fn of_file_or_warn(file: &std::fs::File) -> Option<Self> {
        match Self::of_file(file) {
            Ok(identity) => Some(identity),
            Err(e) => {
                eprintln!(
                    "Failed to stat singleton lock file, state-identity checks disabled: {e}"
                );
                None
            }
        }
    }
}

/// Result of comparing a held lock identity against what is currently on disk.
#[derive(Debug, PartialEq, Eq)]
pub enum LockCheck {
    /// The path still resolves to the identity the daemon is holding open.
    Intact,
    /// Nothing exists at the lock path anymore.
    Missing,
    /// The path now resolves to a different file than the one held open.
    Replaced { found: LockIdentity },
}

/// Compare the held lock's identity with whatever now sits at
/// `<work_dir>/orchestrator.lock`.
///
/// A metadata error other than "not found" (e.g. a transient EACCES) is
/// treated as `Intact` rather than `Missing` — it does not prove the state
/// directory is gone, and killing a healthy daemon over a transient stat
/// failure would be worse than skipping one check.
pub fn check_lock_identity(work_dir: &Path, held: LockIdentity) -> LockCheck {
    let path = work_dir.join(LOCK_FILE);
    match std::fs::metadata(&path) {
        Ok(metadata) => {
            let found = LockIdentity {
                dev: metadata.dev(),
                ino: metadata.ino(),
            };
            if found == held {
                LockCheck::Intact
            } else {
                LockCheck::Replaced { found }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LockCheck::Missing,
        Err(_) => LockCheck::Intact,
    }
}

/// The state directory this process started on is gone or belongs to another
/// daemon. Exit WITHOUT running any cleanup: `DaemonServer`'s `Drop` removes
/// control files by path, and `Orchestrator::run` clears tick/scheduling
/// files by path — every one of those paths now belongs to the daemon that
/// owns the new directory, and touching them would corrupt its state.
pub fn abort_foreign_state(reason: &str) -> ! {
    eprintln!("loom orchestrator: {reason}");
    tracing::error!("{reason}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn identity_of_a_freshly_created_file_is_intact() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(LOCK_FILE);
        let file = File::create(&path).unwrap();
        let held = LockIdentity::of_file(&file).unwrap();

        assert_eq!(check_lock_identity(temp.path(), held), LockCheck::Intact);
    }

    #[test]
    fn removed_lock_file_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(LOCK_FILE);
        let file = File::create(&path).unwrap();
        let held = LockIdentity::of_file(&file).unwrap();

        std::fs::remove_file(&path).unwrap();

        assert_eq!(check_lock_identity(temp.path(), held), LockCheck::Missing);
    }

    #[test]
    fn recreated_lock_file_is_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(LOCK_FILE);
        let file = File::create(&path).unwrap();
        let held = LockIdentity::of_file(&file).unwrap();

        std::fs::remove_file(&path).unwrap();
        // Some filesystems reuse an inode number immediately after it is
        // freed; creating an extra, throwaway file first consumes that
        // number so the recreated lock file is guaranteed a different one.
        let _decoy = File::create(temp.path().join("decoy")).unwrap();
        File::create(&path).unwrap();

        match check_lock_identity(temp.path(), held) {
            LockCheck::Replaced { found } => assert_ne!(found.ino, held.ino),
            other => panic!("expected Replaced, got {other:?}"),
        }
    }
}
