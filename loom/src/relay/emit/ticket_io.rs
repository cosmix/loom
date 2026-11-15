//! Atomically writes one relay ticket into a session's scratch directory.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_replace::publish_in_dir;

const TICKET_MODE: u32 = 0o600;

/// Publish `bytes` as `<scratch_dir>/<id>.req` (mode 0600) through
/// `safe_replace::publish_in_dir`. The scratch directory is resolved once and
/// opened without following links; a planted symlink at the ticket name is
/// replaced, never written through.
pub(super) fn write_ticket(scratch_dir: &Path, id: &str, bytes: &[u8]) -> Result<PathBuf> {
    let name = format!("{id}.req");
    let final_path = scratch_dir.join(&name);
    let dir = scratch_dir
        .canonicalize()
        .map_err(anyhow::Error::from)
        .and_then(|dir| safe_open_dirfd(&dir))
        .with_context(|| format!("failed to open {}", scratch_dir.display()))?;
    publish_in_dir(&dir, OsStr::new(&name), bytes, TICKET_MODE, false)
        .with_context(|| format!("failed to write {}", final_path.display()))?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(names.len(), 1, "no temp file is left");
    }

    #[test]
    fn a_planted_symlink_at_the_ticket_name_is_replaced_not_followed() {
        let root = TempDir::new().unwrap();
        let scratch = root.path().join("scratch");
        fs::create_dir(&scratch).unwrap();
        let victim = root.path().join("victim");
        fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, scratch.join("abc123.req")).unwrap();
        let path = write_ticket(&scratch, "abc123", b"{}").unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
        assert!(fs::symlink_metadata(&path).unwrap().is_file());
        assert_eq!(fs::read(&path).unwrap(), b"{}");
    }
}
