//! Reclaims an unterminated last line of `events.jsonl`.

use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::Path;

use chrono::Utc;

use crate::fs::safe_fs::{safe_create_new_in_workdir, safe_open_dirfd};
use crate::fs::safe_read::open_regular_no_follow;

use super::super::store_io::{io_err, open_append, read_regular};
use super::{parse_events, StoreError, MAX_RECORD_BYTES};

/// Whether `events.jsonl` ends in an unterminated line. Reads one byte, so it is
/// cheap enough to gate work that must not touch a record with a clean journal.
pub fn has_torn_tail(dir: &Path) -> bool {
    let Ok(Some(mut file)) = open_regular_no_follow(dir, "events.jsonl", libc::O_RDONLY) else {
        return false;
    };
    let Ok(len) = file.metadata().map(|metadata| metadata.len()) else {
        return false;
    };
    let mut last = [0u8; 1];
    len > 0
        && file
            .seek(SeekFrom::Start(len - 1))
            .and_then(|_| file.read_exact(&mut last))
            .is_ok_and(|()| last[0] != b'\n')
}

/// Moves an unterminated last line of `events.jsonl` to `events.jsonl.torn-<unix
/// seconds>` and truncates the journal after its last newline. True when it did.
///
/// A crash between a write and its newline leaves such a line, and `read_events`
/// refuses the whole journal for it. The caller must have proven that no supervisor
/// can append any more: the truncation would race a live writer. Every complete line
/// must still parse with a contiguous `seq`; a damaged middle line returns its error
/// and changes nothing. The dropped bytes are always kept in the sidecar as raw
/// bytes, even when they look like a valid record or end inside a multibyte
/// character; only the bytes kept in the journal must be UTF-8.
pub fn repair_torn_tail(dir: &Path) -> Result<bool, StoreError> {
    let Some(bytes) = read_regular(dir, "events.jsonl", MAX_RECORD_BYTES)? else {
        return Ok(false);
    };
    if bytes.last().is_none_or(|last| *last == b'\n') {
        return Ok(false);
    }
    let Some(cut) = bytes.iter().rposition(|byte| *byte == b'\n').map(|i| i + 1) else {
        return Ok(false);
    };
    let kept = std::str::from_utf8(&bytes[..cut])
        .map_err(|error| StoreError::Corrupt(format!("events.jsonl: {error}")))?;
    parse_events(kept)?;
    let name = format!("events.jsonl.torn-{}", Utc::now().timestamp());
    let fd = safe_open_dirfd(dir).map_err(io_err)?;
    safe_create_new_in_workdir(fd.as_raw_fd(), Path::new(&name), &bytes[cut..]).map_err(io_err)?;
    let file = open_append(dir)?;
    file.set_len(cut as u64)?;
    file.sync_all()?;
    Ok(true)
}

#[cfg(test)]
#[path = "store_repair_tests.rs"]
mod tests;
