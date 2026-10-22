//! The Git state a confined command reads in its tree, as a pass-cache input:
//! `HEAD`'s text, which names the checked-out branch. The commit it names,
//! other refs and the index are left out: the content manifest
//! (`cache_manifest`) holds the tree, and a criterion that runs `git` itself
//! is never cached (`cache::is_cacheable`). The file is parsed directly;
//! running `git` would execute commands the repository's config names.

use sha2::Sha256;
use std::path::Path;

use super::cache_fingerprint::hash_field;
use super::confine::GitDirs;
use crate::fs::safe_read::read_to_string_bounded;

/// Bound on `HEAD`: one object id or symbolic ref.
const MAX_REF_BYTES: usize = 4 * 1024;

/// Hash the state of `git`, or a marker for a command with no Git directory.
/// `None` when `HEAD` cannot be read, which makes the run ineligible for the
/// cache.
pub(super) fn hash_git_state(git: Option<&GitDirs>, hasher: &mut Sha256) -> Option<()> {
    let Some(git) = git else {
        hash_field(hasher, b"no-git");
        return Some(());
    };
    let head = read_to_string_bounded(&git.admin, Path::new("HEAD"), MAX_REF_BYTES).ok()?;
    hash_field(hasher, head.as_bytes());
    Some(())
}
