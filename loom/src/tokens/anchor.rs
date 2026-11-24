//! A fingerprint of the bytes a transcript scan has already read, saved with
//! the file's offset so a later scan can tell an appended file from one
//! rewritten in place.

use super::accumulate::FileCursor;
use super::tail::read_range;
use crate::utils::sha256_hex;
use std::io;
use std::path::Path;

/// Bytes hashed at each end of a long prefix. Transcript lines are kilobytes,
/// so a field changed near the start of the last line still changes the
/// fingerprint. A rewrite that changes only bytes outside both windows goes
/// unseen; that limit is deliberate.
pub(super) const ANCHOR_BYTES: u64 = 4096;

/// The fingerprint of the bytes `[0, end)` of the file: the whole range when
/// it is at most `2 * ANCHOR_BYTES`, else its first and last `ANCHOR_BYTES`.
/// `None` when the file is shorter than `end`. A stable hash: the value is
/// saved in the checkpoint and compared by a later process.
pub(super) fn of_prefix(path: &Path, end: u64) -> io::Result<Option<u64>> {
    if end == 0 {
        return Ok(Some(0));
    }
    let bytes = if end <= 2 * ANCHOR_BYTES {
        read_range(path, 0, end)?
    } else {
        let head = read_range(path, 0, ANCHOR_BYTES)?;
        let tail = read_range(path, end - ANCHOR_BYTES, end)?;
        head.zip(tail).map(|(mut head, tail)| {
            head.extend_from_slice(&tail);
            head
        })
    };
    Ok(bytes.map(|bytes| fingerprint(&bytes)))
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let digest = sha256_hex(bytes);
    u64::from_str_radix(&digest[..16], 16).unwrap_or(0)
}

/// Whether the file no longer starts with the bytes `cursor` saw: false for a
/// cursor that has read nothing.
pub(super) fn rewritten(path: &Path, cursor: &FileCursor) -> io::Result<bool> {
    if cursor.offset == 0 {
        return Ok(false);
    }
    Ok(of_prefix(path, cursor.offset)? != Some(cursor.anchor))
}
