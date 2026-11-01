//! Readers for the dispute records under
//! `.loom/work/disputes/<stage>/<n>/{request.md,verdict.md}`, used by the
//! stage's ended rounds (`rounds.rs`).
//!
//! The record formats are written by the dispute pipeline and change with it,
//! so every lookup here is lenient: a missing key or an unreadable file leaves
//! that part out, never fails the status payload.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::fs::safe_read::read_to_string_bounded;
use crate::orchestrator::adjudication::dispute_ids;
use crate::parser::frontmatter::extract_yaml_frontmatter;

/// Longest dispute record read; a record carries evidence and citations, and a
/// larger file is skipped rather than loaded every second.
const MAX_RECORD_BYTES: usize = 1024 * 1024;

pub(super) fn sorted_dispute_ids(stage_dir: &Path) -> Vec<u32> {
    let mut ids = dispute_ids(stage_dir);
    ids.sort_unstable();
    ids
}

pub(super) fn timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

pub(super) fn text(value: Option<&Value>) -> Option<String> {
    value?.as_str().map(str::to_owned)
}

/// The record's frontmatter, or `Null` (every lookup on it misses) when the
/// file is absent, oversized or malformed.
pub(super) fn frontmatter(root: &Path, path: &Path) -> Value {
    let Ok(relative) = path.strip_prefix(root) else {
        return Value::Null;
    };
    read_to_string_bounded(root, relative, MAX_RECORD_BYTES)
        .ok()
        .and_then(|content| extract_yaml_frontmatter(&content).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_fifo_and_symlink_without_reading_them() {
        let root = tempfile::tempdir().unwrap();
        let pipe = root.path().join("request.md");
        nix::unistd::mkfifo(&pipe, nix::sys::stat::Mode::S_IRUSR).unwrap();
        assert!(frontmatter(root.path(), &pipe).is_null());

        fs::remove_file(&pipe).unwrap();
        let target = root.path().join("target.md");
        fs::write(&target, "---\nreason: outside\n---\n").unwrap();
        std::os::unix::fs::symlink(&target, &pipe).unwrap();
        assert!(frontmatter(root.path(), &pipe).is_null());
    }

    #[test]
    fn accepts_regular_record_and_rejects_oversized_record() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("request.md");
        fs::write(&path, "---\nreason: recorded\n---\n").unwrap();
        assert_eq!(frontmatter(root.path(), &path)["reason"], "recorded");

        fs::write(&path, "x".repeat(MAX_RECORD_BYTES + 1)).unwrap();
        assert!(frontmatter(root.path(), &path).is_null());
    }
}
