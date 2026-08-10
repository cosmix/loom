//! No-follow creation and publication of daemon control-plane files.

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

pub(super) fn ensure_private_control_dir(work_dir: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(PRIVATE_DIRECTORY_MODE);
    match builder.create(work_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("failed to create daemon control directory"),
    }

    let directory = crate::fs::safe_fs::safe_open_dirfd(work_dir)
        .context("daemon control directory must be a real directory")?;
    // SAFETY: `directory` is a live descriptor opened without following the
    // final component, and the mode is a fixed owner-only mask.
    if unsafe {
        libc::fchmod(
            directory.as_raw_fd(),
            PRIVATE_DIRECTORY_MODE as libc::mode_t,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error())
            .context("failed to restrict daemon control directory permissions");
    }
    Ok(())
}

/// Atomically replace a control file with content created mode-0600 and
/// `O_EXCL`, never following a destination symlink.
pub(super) fn publish_private_file(work_dir: &Path, relative: &Path, content: &[u8]) -> Result<()> {
    let directory = crate::fs::safe_fs::safe_open_dirfd(work_dir)?;
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("daemon control filename must be valid UTF-8"))?;
    let temporary_name = format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4().simple());
    let temporary = Path::new(&temporary_name);
    crate::fs::safe_fs::safe_create_new_in_workdir(directory.as_raw_fd(), temporary, content)?;

    let result =
        crate::fs::safe_fs::safe_rename_in_workdir(directory.as_raw_fd(), temporary, relative);
    if result.is_err() {
        let _ = crate::fs::safe_fs::safe_remove_in_workdir(directory.as_raw_fd(), temporary);
    }
    result
}

/// Open a daemon-owned output file without following any symlink.
pub(super) fn open_private_output(work_dir: &Path, relative: &Path) -> Result<File> {
    let directory = crate::fs::safe_fs::safe_open_dirfd(work_dir)?;
    let descriptor: OwnedFd = crate::fs::safe_fs::open_safely(
        directory.as_raw_fd(),
        relative,
        libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        PRIVATE_FILE_MODE,
    )?;
    let file = File::from(descriptor);
    file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    Ok(file)
}

pub(super) fn remove_control_file(work_dir: &Path, relative: &Path) -> Result<()> {
    let directory = crate::fs::safe_fs::safe_open_dirfd(work_dir)?;
    match crate::fs::safe_fs::safe_remove_in_workdir(directory.as_raw_fd(), relative) {
        Ok(()) => Ok(()),
        Err(error) if is_not_found(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{symlink, MetadataExt};

    #[test]
    fn control_directory_is_created_owner_only() {
        let parent = tempfile::tempdir().unwrap();
        let work_dir = parent.path().join(".work");

        ensure_private_control_dir(&work_dir).unwrap();

        assert_eq!(fs::metadata(work_dir).unwrap().mode() & 0o777, 0o700);
    }

    #[test]
    fn publication_replaces_leaf_symlink_without_touching_target() {
        let work_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"outside").unwrap();
        symlink(outside.path(), work_dir.path().join("admin.token")).unwrap();

        publish_private_file(work_dir.path(), Path::new("admin.token"), b"secret").unwrap();

        assert_eq!(
            fs::read(work_dir.path().join("admin.token")).unwrap(),
            b"secret"
        );
        assert_eq!(fs::read(outside.path()).unwrap(), b"outside");
        assert_eq!(
            fs::metadata(work_dir.path().join("admin.token"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn output_open_refuses_leaf_symlink() {
        let work_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"outside").unwrap();
        symlink(outside.path(), work_dir.path().join("orchestrator.log")).unwrap();

        assert!(open_private_output(work_dir.path(), Path::new("orchestrator.log")).is_err());
        assert_eq!(fs::read(outside.path()).unwrap(), b"outside");
    }

    #[test]
    fn private_output_is_mode_0600_before_use() {
        let work_dir = tempfile::tempdir().unwrap();
        let mut output =
            open_private_output(work_dir.path(), Path::new("orchestrator.log")).unwrap();
        output.write_all(b"log").unwrap();

        let metadata = fs::metadata(work_dir.path().join("orchestrator.log")).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
    }
}
