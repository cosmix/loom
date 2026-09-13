//! Enumerating the entries the daemon's drain should apply next
//! (`doc/plans/PLAN-loom-state-confinement.md` section 8, drain step 3).

use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::paths::session_inbox;
use crate::daemon::MAX_REQUEST_BYTES;
use crate::validation::validate_id;

/// Entries safe to drain, and the names of `*.json` entries skipped because
/// they did not look like a relay hook's own write (a planted symlink, a
/// hard-linked file, or something oversized).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingEntries {
    pub entries: Vec<PathBuf>,
    pub skipped: Vec<String>,
}

/// List `<id>.json` files under `session_id`'s inbox, sorted by file name.
/// Only regular, single-link, non-symlink files within
/// [`crate::daemon::MAX_REQUEST_BYTES`] are returned as entries; every other
/// `*.json` name is reported in `skipped` so the drain can refuse it rather
/// than silently ignore it. A non-`.json` name is neither an entry nor a
/// skip — it is not part of the relay protocol's own naming at all.
pub fn pending_entries(work_dir: &Path, session_id: &str) -> Result<PendingEntries> {
    validate_id(session_id).context("invalid inbox session id")?;
    let dir = session_inbox(work_dir, session_id)?;

    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(read_dir) => read_dir
            .map(|entry| {
                entry
                    .with_context(|| format!("failed to read entry in {}", dir.display()))
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
            })
            .collect::<Result<_>>()?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to list {}", dir.display()))
        }
    };
    names.retain(|name| name.ends_with(".json"));
    names.sort();

    let mut result = PendingEntries::default();
    for name in names {
        let path = dir.join(&name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.file_type().is_file()
                    && metadata.nlink() == 1
                    && metadata.len() <= MAX_REQUEST_BYTES as u64 =>
            {
                result.entries.push(path);
            }
            _ => result.skipped.push(name),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_dir(work_dir: &Path, session_id: &str) -> PathBuf {
        let dir = work_dir.join("inbox").join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_session_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert!(result.entries.is_empty());
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn lists_regular_json_files_sorted_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = session_dir(tmp.path(), "session-1");
        std::fs::write(dir.join("b.json"), b"{}").unwrap();
        std::fs::write(dir.join("a.json"), b"{}").unwrap();

        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert_eq!(result.entries, vec![dir.join("a.json"), dir.join("b.json")]);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn skips_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = session_dir(tmp.path(), "session-1");
        let target = dir.join("real.json");
        std::fs::write(&target, b"{}").unwrap();
        std::os::unix::fs::symlink(&target, dir.join("link.json")).unwrap();

        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert_eq!(result.entries, vec![target]);
        assert_eq!(result.skipped, vec!["link.json".to_string()]);
    }

    #[test]
    fn skips_a_hard_linked_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = session_dir(tmp.path(), "session-1");
        let original = dir.join("a.json");
        std::fs::write(&original, b"{}").unwrap();
        std::fs::hard_link(&original, dir.join("b.json")).unwrap();

        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert!(result.entries.is_empty());
        assert_eq!(result.skipped.len(), 2);
    }

    #[test]
    fn skips_an_oversized_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = session_dir(tmp.path(), "session-1");
        let big = vec![b'x'; MAX_REQUEST_BYTES + 1];
        std::fs::write(dir.join("big.json"), &big).unwrap();

        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert!(result.entries.is_empty());
        assert_eq!(result.skipped, vec!["big.json".to_string()]);
    }

    #[test]
    fn ignores_a_non_json_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = session_dir(tmp.path(), "session-1");
        std::fs::write(dir.join("note.txt"), b"hello").unwrap();

        let result = pending_entries(tmp.path(), "session-1").unwrap();
        assert!(result.entries.is_empty());
        assert!(result.skipped.is_empty());
    }
}
