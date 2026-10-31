//! Bounded reads of package files, shared by kind detection and runner selection.
//!
//! Every read names an anchor `root` and resolves the path beneath it through
//! `crate::fs::safe_read`, which opens each component below `root` without
//! following a symlink. A symlink, a non-regular file, a path outside `root`,
//! or a file over `MAX_FILE_BYTES` reads as absent, so a checkout cannot
//! redirect discovery outside itself or stall a prompt hook on a huge file.

use std::path::Path;

pub(super) use crate::language::evidence::{files_where, has_dependency, read_json};
use regex::Regex;
use serde_json::Value;

use super::markers::regular_file;
use crate::fs::safe_read;

const MAX_FILE_BYTES: usize = 256 * 1024;
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

/// Whether `package_dir/package.json` (beneath `root`) lists a `dependencies` or
/// `devDependencies` entry: a JS package needs an install only then. A missing,
/// unreadable or unparsable manifest counts as declaring one, so a caller that
/// skips dependency-free packages never skips a package it could not read.
pub(crate) fn declares_dependencies(root: &Path, package_dir: &Path) -> bool {
    let Some(manifest) = read_json(root, &package_dir.join("package.json")) else {
        return true;
    };
    ["dependencies", "devDependencies"].iter().any(|key| {
        manifest
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(|entries| !entries.is_empty())
    })
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
