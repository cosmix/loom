//! The worktree walk behind [`super::changes::special_files`]: every FIFO,
//! socket and device node, found without following a symlink.

use anyhow::Result;
use std::collections::BTreeSet;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{list_dir_no_follow, EntryKind};

/// Worktree-relative paths of every FIFO, socket and device node beneath
/// `worktree_root`, skipping the `ignored` paths and every `.git` directory.
///
/// Each directory is opened from the worktree root's descriptor with a
/// symlink refused at every component, then listed from that descriptor
/// ([`list_dir_no_follow`]). A directory swapped for a symlink after its
/// parent was listed fails that open, and the walk fails with it: nothing
/// outside the worktree is listed, and nothing inside is skipped silently.
pub(super) fn special_paths(
    worktree_root: &Path,
    ignored: &BTreeSet<PathBuf>,
) -> Result<BTreeSet<PathBuf>> {
    let root = safe_open_dirfd(worktree_root)?;
    let mut found = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(dir) = pending.pop() {
        let entries = list_dir_no_follow(&root, &dir).map_err(|error| refusal(&dir, error))?;
        for (name, kind) in entries {
            let path = dir.join(&name);
            if ignored.contains(&path) {
                continue;
            }
            match kind {
                EntryKind::Directory if name != ".git" => pending.push(path),
                EntryKind::Special => {
                    found.insert(path);
                }
                _ => {}
            }
        }
    }
    Ok(found)
}

/// Why `dir` could not be listed, naming a swap for a symlink or a
/// non-directory (`ELOOP`, or `ENOTDIR` on macOS) as such.
fn refusal(dir: &Path, error: anyhow::Error) -> anyhow::Error {
    let shown = escaped(dir.as_os_str().as_bytes());
    let swapped = error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|cause| matches!(cause.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)));
    if swapped {
        error.context(format!(
            "refusing to list '{shown}' in the worktree: it is no longer a directory, \
             and a symlink in its place is not followed"
        ))
    } else {
        error.context(format!("cannot list '{shown}' in the worktree"))
    }
}

/// `bytes` as message text that maps back to them: valid UTF-8 as is, except
/// a backslash (doubled) and a control character (`\n`, `\u{1b}`), and every
/// byte that is not UTF-8 as `\xNN`.
pub(super) fn escaped(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for character in chunk.valid().chars() {
            if character == '\\' || character.is_control() {
                text.extend(character.escape_default());
            } else {
                text.push(character);
            }
        }
        for byte in chunk.invalid() {
            text.push_str(&format!("\\x{byte:02x}"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_keeps_every_byte_distinguishable() {
        assert_eq!(escaped("src/lib.rs".as_bytes()), "src/lib.rs");
        assert_eq!(escaped("dé/p".as_bytes()), "dé/p");
        assert_eq!(escaped(b"a\\b\ncd\xff"), "a\\\\b\\ncd\\xff");
        assert_eq!(escaped(b"\\xff"), "\\\\xff");
    }

    /// The walk popped `dir`, and a symlink now stands in its place: the
    /// listing is refused, and the refusal says why.
    #[test]
    fn a_directory_swapped_for_a_symlink_is_refused_by_name() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();
        let root_fd = safe_open_dirfd(root.path()).unwrap();
        let dir = Path::new("dir");

        let error = list_dir_no_follow(&root_fd, dir).map_err(|error| refusal(dir, error));

        let message = format!("{:#}", error.unwrap_err());
        assert!(
            message.starts_with("refusing to list 'dir' in the worktree"),
            "{message}"
        );
    }
}
