//! Binary installation with atomic rollback mechanism.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

/// Install binary with atomic rollback mechanism.
/// Writes new binary to temp location, backs up current, installs new with rollback on failure.
pub(crate) fn install_binary(new_binary: &[u8], current_exe: &Path) -> Result<()> {
    let parent = current_exe
        .parent()
        .context("Binary has no parent directory")?;

    // Write new binary to unpredictable temp path (SEC-MED-06)
    let mut staging =
        NamedTempFile::new_in(parent).context("Failed to create staging temp file")?;
    staging
        .write_all(new_binary)
        .context("Failed to write new binary")?;
    staging
        .as_file()
        .sync_all()
        .context("Failed to sync new binary to disk")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permissions on new binary")?;
    }

    // Create unpredictable backup path for rollback (SEC-MED-06)
    let backup = NamedTempFile::new_in(parent).context("Failed to create backup temp file")?;
    let backup_path = backup.into_temp_path();

    // Backup current binary (atomically replaces the empty temp file)
    fs::rename(current_exe, &*backup_path).context("Failed to backup current binary")?;

    // Install new binary - with rollback on failure
    let staging_path = staging.into_temp_path();
    if let Err(e) = fs::rename(&*staging_path, current_exe) {
        // Attempt rollback
        if let Err(rollback_err) = fs::rename(&*backup_path, current_exe) {
            bail!(
                "CRITICAL: Update failed and rollback failed!\n\
                 Update error: {}\n\
                 Rollback error: {}\n\
                 Manual recovery needed: copy {} to {}",
                e,
                rollback_err,
                backup_path.display(),
                current_exe.display()
            );
        }
        return Err(e.into());
    }

    // Success - backup auto-deleted by TempPath drop
    Ok(())
}
