//! Binary installation with atomic rollback mechanism.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Install binary with atomic rollback mechanism.
/// Writes new binary to temp location, backs up current, installs new with rollback on failure.
pub(crate) fn install_binary(new_binary: &[u8], current_exe: &Path) -> Result<()> {
    let backup_path = current_exe.with_extension("backup");
    let new_path = current_exe.with_extension("new");

    // Clean up any leftover files from previous failed updates
    if backup_path.exists() {
        fs::remove_file(&backup_path).ok();
    }
    if new_path.exists() {
        fs::remove_file(&new_path).ok();
    }

    // Write new binary to temp location
    let mut file = File::create(&new_path).context("Failed to create new binary file")?;
    file.write_all(new_binary)
        .context("Failed to write new binary")?;
    file.sync_all()
        .context("Failed to sync new binary to disk")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new_path, fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permissions on new binary")?;
    }

    // Backup current binary
    fs::rename(current_exe, &backup_path).context("Failed to backup current binary")?;

    // Install new binary - with rollback on failure
    if let Err(e) = fs::rename(&new_path, current_exe) {
        // Attempt rollback
        if let Err(rollback_err) = fs::rename(&backup_path, current_exe) {
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

    // Success - remove backup
    let _ = fs::remove_file(&backup_path);
    Ok(())
}
