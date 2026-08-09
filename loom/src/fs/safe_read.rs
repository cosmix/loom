//! Bounded, dirfd-relative reads that reject symlinks at every path component.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Take};
use std::os::fd::AsRawFd;
use std::path::Path;

use super::safe_fs::{open_safely, safe_open_dirfd};

/// Read at most `max_bytes` from a regular file beneath `root`.
///
/// `relative` must not be absolute or contain `..`. The kernel opens every
/// component with no-follow semantics, so an attacker cannot substitute a
/// symlink between validation and use.
pub fn read_bounded(root: &Path, relative: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let root_fd = safe_open_dirfd(root)?;
    let file_fd = open_safely(root_fd.as_raw_fd(), relative, libc::O_RDONLY, 0)
        .with_context(|| format!("Refusing unsafe read of {}", relative.display()))?;
    let file = File::from(file_fd);
    let metadata = file.metadata().context("Failed to inspect opened file")?;
    if !metadata.is_file() {
        bail!("{} is not a regular file", relative.display());
    }
    if metadata.len() > max_bytes as u64 {
        bail!(
            "{} exceeds the {} byte verification limit",
            relative.display(),
            max_bytes
        );
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut reader: Take<File> = file.take(limit);
    reader
        .read_to_end(&mut bytes)
        .context("Failed to read opened file")?;
    if bytes.len() > max_bytes {
        bail!(
            "{} grew beyond the {} byte verification limit while reading",
            relative.display(),
            max_bytes
        );
    }
    Ok(bytes)
}

/// Read a bounded UTF-8 file beneath `root` without following symlinks.
pub fn read_to_string_bounded(root: &Path, relative: &Path, max_bytes: usize) -> Result<String> {
    let bytes = read_bounded(root, relative, max_bytes)?;
    String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", relative.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_leaf_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();

        assert!(read_bounded(root.path(), Path::new("link"), 1024).is_err());
    }

    #[test]
    fn rejects_intermediate_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();

        assert!(read_bounded(root.path(), Path::new("dir/secret"), 1024).is_err());
    }
}
