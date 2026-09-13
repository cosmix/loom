//! Opening what sits in an inbox or scratch directory without trusting it: no
//! symlink is followed, no FIFO blocks the daemon, and only a regular
//! single-link file within the size cap is ever read.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::daemon::MAX_REQUEST_BYTES;
use crate::relay::{InboxEntry, RequestKind};

/// One entry file, read.
pub(super) enum ReadEntry {
    Entry(InboxEntry),
    /// Refused as malformed; `kind` is set when the bytes still name one.
    Malformed {
        kind: Option<RequestKind>,
        reason: String,
    },
}

/// A file opened safely: its bytes, or why it was not read.
pub(super) enum Regular {
    Bytes(Vec<u8>),
    Refused(String),
}

/// Read and decode one `<id>.json`. `Err` is an I/O failure only; everything
/// the file itself gets wrong comes back as [`ReadEntry::Malformed`].
pub(super) fn read_entry(path: &Path) -> Result<ReadEntry> {
    let bytes = match read_regular(path, MAX_REQUEST_BYTES)? {
        Regular::Bytes(bytes) => bytes,
        Regular::Refused(reason) => {
            return Ok(ReadEntry::Malformed { kind: None, reason });
        }
    };
    Ok(match InboxEntry::decode(&bytes) {
        Ok(entry) => ReadEntry::Entry(entry),
        Err(error) => ReadEntry::Malformed {
            kind: kind_hint(&bytes),
            reason: format!("malformed entry: {error:#}"),
        },
    })
}

/// Open `path` without following a symlink or blocking on a FIFO, and read it
/// only when it is a regular, single-link file of at most `cap` bytes.
pub(super) fn read_regular(path: &Path, cap: usize) -> Result<Regular> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
            return Ok(Regular::Refused("the entry is a symlink".to_string()));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to open {}", path.display()))
        }
    };
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to stat {}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.nlink() != 1 || metadata.len() > cap as u64 {
        return Ok(Regular::Refused(
            "the entry is not a regular single-link file within the size cap".to_string(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() > cap {
        return Ok(Regular::Refused(
            "the entry grew past the size cap".to_string(),
        ));
    }
    Ok(Regular::Bytes(bytes))
}

/// The `kind` a malformed entry still names, so its refusal can be recorded
/// under the right kind.
fn kind_hint(bytes: &[u8]) -> Option<RequestKind> {
    serde_json::from_slice::<Value>(bytes)
        .ok()?
        .get("kind")?
        .as_str()?
        .parse()
        .ok()
}

/// Whether `root/name` is a plain session directory: a valid session id, and a
/// directory that opens without following a symlink.
pub(super) fn check_session_dir(root: &Path, name: &str) -> Result<(), String> {
    crate::validation::validate_id(name).map_err(|error| format!("{error:#}"))?;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root.join(name))
        .map(drop)
        .map_err(|error| format!("cannot open it as a directory without following links: {error}"))
}

/// Remove one directory entry by name: a file, a symlink or a FIFO, never
/// what a symlink points at. Already gone is success.
pub(super) fn remove_name(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

/// Remove a directory tree, or just the name when `path` is not a real
/// directory (a symlink is removed, never followed). Already gone is success.
pub(super) fn remove_tree(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove {}", path.display())),
        Ok(_) => remove_name(path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to stat {}", path.display())),
    }
}
