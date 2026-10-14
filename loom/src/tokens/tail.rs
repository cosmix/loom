//! Incremental reads: the complete lines a file gained since a saved offset.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Longest line handed over whole. A longer one is skipped and reported as
/// [`Line::TooLong`]; a transcript line is a few KB, or a few MB with a large
/// tool result in it.
pub(super) const MAX_LINE_BYTES: usize = 32 * 1024 * 1024;

/// What a stat of a watched file reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStat {
    pub(super) len: u64,
    pub(super) inode: u64,
}

/// One complete line of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Line<'a> {
    /// The line without its `\n`.
    Text(&'a str),
    /// The line was over [`MAX_LINE_BYTES`] and was skipped unread.
    TooLong,
}

/// Size and inode of the regular file at `path`; `None` when it is missing, a
/// symlink (the read opens no-follow), or not a regular file.
pub(super) fn stat(path: &Path) -> Option<FileStat> {
    let metadata = fs::symlink_metadata(path).ok()?;
    metadata.is_file().then(|| FileStat {
        len: metadata.len(),
        inode: metadata.ino(),
    })
}

/// Calls `on_line` with each complete line between `*offset` and `len`, moving
/// `*offset` past each line as it is handed over, so an error part way leaves
/// `*offset` at the first line not consumed. An unfinished last line is left
/// for a later read. Stops between lines once `stop` is set. `O_NOFOLLOW`
/// refuses a symlink and `O_NONBLOCK` keeps a FIFO put in the file's place
/// after [`stat`] from blocking the open.
pub(super) fn read_lines(
    path: &Path,
    offset: &mut u64,
    len: u64,
    stop: &AtomicBool,
    mut on_line: impl FnMut(Line<'_>),
) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    file.seek(SeekFrom::Start(*offset))?;
    let mut reader = BufReader::new(file.take(len.saturating_sub(*offset)));
    let mut line = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let Some((consumed, cut)) = next_line(&mut reader, &mut line, MAX_LINE_BYTES)? else {
            return Ok(());
        };
        *offset += consumed;
        if cut {
            on_line(Line::TooLong);
        } else {
            on_line(Line::Text(&String::from_utf8_lossy(&line)));
        }
    }
    Ok(())
}

/// Reads the next `\n`-terminated line into `line` (without the `\n`), keeping
/// at most `max` bytes of it and consuming the rest unbuffered. Returns the
/// bytes consumed, newline included, and whether the line was cut at `max`;
/// `None` at an unfinished last line, which the caller must not count as read.
fn next_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    max: usize,
) -> io::Result<Option<(u64, bool)>> {
    line.clear();
    let mut consumed = 0u64;
    let mut cut = false;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(None);
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let body = &chunk[..newline.unwrap_or(chunk.len())];
        let room = max.saturating_sub(line.len());
        line.extend_from_slice(&body[..body.len().min(room)]);
        cut |= body.len() > room;
        let used = newline.map_or(chunk.len(), |at| at + 1);
        reader.consume(used);
        consumed += used as u64;
        if newline.is_some() {
            return Ok(Some((consumed, cut)));
        }
    }
}

#[cfg(test)]
#[path = "tail_tests.rs"]
mod tests;
