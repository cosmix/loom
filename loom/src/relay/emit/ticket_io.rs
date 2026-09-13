//! Atomically writes one relay ticket into a session's scratch directory.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const TICKET_MODE: u32 = 0o600;

/// Write `bytes` to `<scratch_dir>/.<id>.tmp` (`O_CREAT|O_EXCL`, mode 0600),
/// fsync it, then rename it to `<scratch_dir>/<id>.req`.
pub(super) fn write_ticket(scratch_dir: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf> {
    let tmp_path = scratch_dir.join(format!(".{id}.tmp"));
    let final_path = scratch_dir.join(format!("{id}.req"));

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(TICKET_MODE)
        .open(&tmp_path)
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", tmp_path.display()))?;
    drop(file);

    fs::rename(&tmp_path, &final_path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            final_path.display()
        )
    })?;

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn writes_the_ticket_at_mode_0600_under_the_final_name() {
        let dir = TempDir::new().unwrap();
        let path = write_ticket(dir.path(), "abc123", b"{\"v\":1}").unwrap();
        assert_eq!(path, dir.path().join("abc123.req"));
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, TICKET_MODE);
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":1}");
        assert!(!dir.path().join(".abc123.tmp").exists());
    }
}
