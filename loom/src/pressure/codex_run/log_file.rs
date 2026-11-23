//! The Codex output log: created without following a link, read back as a bounded tail, and
//! scanned for the account-level "model unavailable" error.

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Bytes from the end of the log scanned for an unavailable-model error: the whole log is never
/// loaded. Revisit if Codex ever prints that error more than this far from the end of a failed
/// attempt's log.
pub(super) const LOG_SCAN_BYTES: u64 = 256 * 1024;

/// Phrases Codex and the API print when the account cannot use a model,
/// matched case-insensitively.
const UNAVAILABLE_PHRASES: [&str; 4] = [
    "model is not supported",
    "model not found",
    "model_not_found",
    "does not exist",
];

/// The first error line naming `model` together with an unavailability
/// phrase. Tool output and model prose can quote those phrases, so only
/// `ERROR` lines count. Requiring both on one line also excludes Codex's
/// harmless metadata warning and unrelated errors.
pub(super) fn unavailable_line<'a>(log: &'a str, model: &str) -> Option<&'a str> {
    log.lines().find(|line| {
        let lower = line.to_lowercase();
        line.trim_start().starts_with("ERROR")
            && line.contains(model)
            && UNAVAILABLE_PHRASES.iter().any(|p| lower.contains(p))
    })
}

/// Create the log fresh (mode 0600). A regular file left by an earlier attempt is removed; any
/// other entry (a symlink, a directory) is refused, so the open can never truncate a link's
/// target.
///
/// `create_new` is `O_EXCL`: an entry planted between the removal and the open fails the open
/// instead of being followed, and `O_NOFOLLOW` covers the final component besides.
pub(super) fn create_log(path: &Path) -> Result<File> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_file() => std::fs::remove_file(path)
            .with_context(|| format!("cannot replace the codex log {}", path.display()))?,
        Ok(_) => bail!(
            "refusing the codex log {}: it is not a regular file",
            path.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("cannot inspect {}", path.display()))
        }
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("failed to create codex log {}", path.display()))
}

/// The last `max_bytes` of the log through a no-follow open of regular files only; `None` when
/// it cannot be read.
pub(crate) fn tail(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let name = path.file_name()?;
    crate::fs::safe_read::read_tail(&parent, Path::new(name), max_bytes).ok()
}
