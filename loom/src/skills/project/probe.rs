//! Bounded reads of package files, shared by kind detection and runner selection.
//!
//! Every read names an anchor `root` and resolves the path beneath it through
//! `crate::fs::safe_read`, which opens each component below `root` without
//! following a symlink. A symlink, a non-regular file, a path outside `root`,
//! or a file over `MAX_FILE_BYTES` reads as absent, so a checkout cannot
//! redirect discovery outside itself or stall a prompt hook on a huge file.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

use super::markers::regular_file;
use crate::fs::safe_read;

const MAX_FILE_BYTES: usize = 256 * 1024;
/// Entries examined per directory listing; discovery runs on every prompt.
const MAX_LISTED_ENTRIES: usize = 4_096;

pub(super) fn read_json(root: &Path, path: &Path) -> Option<Value> {
    serde_json::from_slice(&read_bounded(root, path)?).ok()
}

/// Whether a line of `path` that is not a `#` comment matches `pattern`; a
/// missing, unreadable or oversized file never matches.
pub(super) fn has_line(root: &Path, path: &Path, pattern: &Regex) -> bool {
    read_bounded(root, path).is_some_and(|bytes| {
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .any(|line| pattern.is_match(line))
    })
}

/// Whether any of the manifest's `sections` (objects keyed by package name) lists `name`.
pub(super) fn has_dependency(manifest: &Value, sections: &[&str], name: &str) -> bool {
    sections.iter().any(|section| {
        manifest
            .get(section)
            .and_then(Value::as_object)
            .is_some_and(|deps| deps.contains_key(name))
    })
}

/// Regular files directly inside `dir` whose name satisfies `matches`.
pub(super) fn files_where(
    dir: &Path,
    matches: impl Fn(&str) -> bool,
) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .take(MAX_LISTED_ENTRIES)
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(move |entry| matches(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
}

/// The bytes of `path`, which must lie beneath `root`. The `lstat` check only
/// keeps a FIFO from blocking the open; `safe_read` is what refuses symlinks.
fn read_bounded(root: &Path, path: &Path) -> Option<Vec<u8>> {
    let relative = path.strip_prefix(root).ok()?;
    if !regular_file(path) {
        return None;
    }
    safe_read::read_bounded(root, relative, MAX_FILE_BYTES).ok()
}
