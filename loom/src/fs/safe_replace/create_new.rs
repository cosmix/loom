//! Publication that never replaces: the first publisher of a name wins.
//!
//! The bytes go to a unique temporary file in the directory (created exclusively, never
//! through a link, synced), which is then hard-linked to the final name. `linkat` fails with
//! `EEXIST` instead of replacing, so two publishers of one name cannot both succeed and a
//! reader never sees a partial file. The temporary name is removed either way.

use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, OwnedFd};

use anyhow::{Context, Result};

use super::{c_name, create_temp, fill_with_mode, sync_dir, temp_name, unlink_at};

/// Publish `bytes` as `name` in `parent` with permission bits `mode` unless `name` exists.
/// `Ok(true)` when this call wrote it, `Ok(false)` when an entry (of any kind) was already
/// there and was left untouched. A directory sync failure after the link is logged: the
/// file is published.
pub(crate) fn publish_new_in_dir(
    parent: &OwnedFd,
    name: &OsStr,
    bytes: &[u8],
    mode: u32,
) -> Result<bool> {
    let name = c_name(name.as_bytes())?;
    let tmp = c_name(&temp_name(name.as_bytes()))?;
    let file = create_temp(parent, &tmp, mode).context("cannot create the temporary file")?;
    let linked =
        fill_with_mode(file, mode, true, bytes).and_then(|()| link_new(parent, &tmp, &name));
    unlink_at(parent, &tmp);
    let written = linked?;
    if written {
        if let Err(error) = sync_dir(parent) {
            tracing::warn!("cannot sync directory after publication: {error:#}");
        }
    }
    Ok(written)
}

/// `linkat(2)` of `from` to `to` in one directory; `Ok(false)` on `EEXIST`.
fn link_new(parent: &OwnedFd, from: &std::ffi::CString, to: &std::ffi::CString) -> Result<bool> {
    let dir = parent.as_raw_fd();
    // SAFETY: both names are NUL-terminated and resolved in the same open directory fd; the
    // flags are 0, so a symlink named `from` would be linked as itself, never followed.
    let rc = unsafe { libc::linkat(dir, from.as_ptr(), dir, to.as_ptr(), 0) };
    if rc == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EEXIST) {
        return Ok(false);
    }
    Err(error).context("cannot link the temporary file to its name")
}

#[cfg(test)]
mod tests {
    use std::os::unix::io::OwnedFd;

    use super::publish_new_in_dir;

    fn open(dir: &std::path::Path) -> OwnedFd {
        crate::fs::safe_fs::safe_open_dirfd(dir).unwrap()
    }

    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_first_publisher_wins_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let fd = open(dir.path());
        assert!(publish_new_in_dir(&fd, "a.json".as_ref(), b"first", 0o600).unwrap());
        assert!(!publish_new_in_dir(&fd, "a.json".as_ref(), b"second", 0o600).unwrap());
        assert_eq!(std::fs::read(dir.path().join("a.json")).unwrap(), b"first");
        assert_eq!(names(dir.path()), ["a.json"]);
    }

    #[test]
    fn a_planted_symlink_is_not_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("victim");
        std::os::unix::fs::symlink(&target, dir.path().join("a.json")).unwrap();
        let fd = open(dir.path());
        assert!(!publish_new_in_dir(&fd, "a.json".as_ref(), b"data", 0o600).unwrap());
        assert!(!target.exists());
    }

    #[test]
    fn the_file_has_the_requested_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fd = open(dir.path());
        publish_new_in_dir(&fd, "a.json".as_ref(), b"x", 0o600).unwrap();
        let mode = std::fs::metadata(dir.path().join("a.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
