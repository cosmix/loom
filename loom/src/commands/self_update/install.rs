//! Binary installation by atomic replacement.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

/// Stage the new binary beside the current one, then atomically replace it.
pub(crate) fn install_binary(new_binary: &[u8], current_exe: &Path) -> Result<()> {
    install_with_replace(new_binary, current_exe, |from, to| fs::rename(from, to))
}

pub(super) fn install_with_replace(
    new_binary: &[u8],
    current_exe: &Path,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
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

    // The original stays at its public path until the replacement succeeds.
    let staging_path = staging.into_temp_path();
    replace(&staging_path, current_exe).context("Failed to replace current binary")
}
