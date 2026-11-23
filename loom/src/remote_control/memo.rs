//! A one-entry memo for a probe of the claude binary.
//!
//! The answer is kept while the binary is the same file (path, length, modification time) and,
//! for an answer that depends on the login too, until its time to live passes. A replaced binary
//! or an expired answer is probed again, so a long-running daemon follows an upgrade or a login.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use super::locked;

/// The identity of the probed file; a failed `metadata` gives length 0 and no time.
#[derive(PartialEq, Eq)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl Key {
    fn of(binary: &Path) -> Self {
        let metadata = std::fs::metadata(binary).ok();
        Self {
            path: binary.to_path_buf(),
            len: metadata.as_ref().map_or(0, std::fs::Metadata::len),
            modified: metadata.and_then(|m| m.modified().ok()),
        }
    }
}

struct Entry<T> {
    key: Key,
    taken: Instant,
    value: T,
}

pub(super) struct Memo<T> {
    slot: Mutex<Option<Entry<T>>>,
}

impl<T: Clone> Memo<T> {
    pub(super) const fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    /// The stored value when `binary` is unchanged and, with a `ttl`, younger than it at `now`;
    /// otherwise `probe`'s result, which replaces the entry. The lock is held while probing, so
    /// a second caller waits for the first instead of probing again.
    pub(super) fn get(
        &self,
        binary: &Path,
        ttl: Option<Duration>,
        now: Instant,
        probe: impl FnOnce() -> T,
    ) -> T {
        let key = Key::of(binary);
        let mut slot = locked(&self.slot);
        if let Some(entry) = slot.as_ref() {
            let fresh = ttl.is_none_or(|ttl| now.saturating_duration_since(entry.taken) < ttl);
            if entry.key == key && fresh {
                return entry.value.clone();
            }
        }
        let value = probe();
        *slot = Some(Entry {
            key,
            taken: now,
            value: value.clone(),
        });
        value
    }
}
