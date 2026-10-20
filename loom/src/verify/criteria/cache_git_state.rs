//! The Git state a confined command reads in its tree, as a pass-cache input:
//! `HEAD`, the object id its branch names, and the index. Objects need no
//! hashing, being content-addressed. Files are parsed directly; running `git`
//! would execute commands the repository's config names.

use sha2::Sha256;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::{Component, Path};

use super::cache_fingerprint::{hash_field, hash_regular_file};
use super::confine::GitDirs;
use crate::fs::safe_read::{is_not_found, open_regular_no_follow, read_to_string_bounded};

/// Bound on `HEAD` or a loose ref: one object id or symbolic ref.
const MAX_REF_BYTES: usize = 4 * 1024;
/// Bound on `packed-refs`, scanned line by line.
const MAX_PACKED_REFS_BYTES: u64 = 16 * 1024 * 1024;

/// Hash the state of `git`, or a marker for a command with no Git directory.
/// `None` when the state cannot be read completely, which makes the run
/// ineligible for the cache.
pub(super) fn hash_git_state(
    git: Option<&GitDirs>,
    remaining: &mut u64,
    hasher: &mut Sha256,
) -> Option<()> {
    let Some(git) = git else {
        hash_field(hasher, b"no-git");
        return Some(());
    };
    let head = read_to_string_bounded(&git.admin, Path::new("HEAD"), MAX_REF_BYTES).ok()?;
    hash_field(hasher, head.as_bytes());
    if let Some(name) = head.strip_prefix("ref:") {
        hash_field(hasher, resolve_ref(&git.common, name.trim())?.as_bytes());
    }
    let index = git.admin.join("index");
    match index.symlink_metadata() {
        Ok(metadata) if metadata.is_file() => {
            hash_field(hasher, b"index");
            hash_regular_file(&index, &metadata, remaining, hasher)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            hash_field(hasher, b"no-index");
            Some(())
        }
        Ok(_) | Err(_) => None,
    }
}

/// What the branch `name` holds: its loose ref file in `common`, else its
/// `packed-refs` line. A name outside `refs/` or with `..` is refused.
fn resolve_ref(common: &Path, name: &str) -> Option<String> {
    let relative = Path::new(name);
    let normal = relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !name.starts_with("refs/") || !normal {
        return None;
    }
    match read_to_string_bounded(common, relative, MAX_REF_BYTES) {
        Ok(value) => Some(value.trim().to_string()),
        Err(error) if is_not_found(&error) => packed_ref(common, name),
        Err(_) => None,
    }
}

/// `name`'s object id in `common/packed-refs`, whose ref lines read
/// `<oid> <name>`.
fn packed_ref(common: &Path, name: &str) -> Option<String> {
    let file = open_regular_no_follow(common, "packed-refs", libc::O_RDONLY).ok()??;
    BufReader::new(file.take(MAX_PACKED_REFS_BYTES))
        .lines()
        .map_while(Result::ok)
        .find_map(|line| {
            let (oid, named) = line.split_once(' ')?;
            (named == name).then(|| oid.to_string())
        })
}
