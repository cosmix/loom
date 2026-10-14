//! Incremental reads: the complete lines a file gained since a saved offset.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

/// What a stat of a watched file reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStat {
    pub(super) len: u64,
    pub(super) inode: u64,
}

/// Size and inode of the regular file at `path`, following symlinks; `None`
/// when it is missing or is not a regular file.
pub(super) fn stat(path: &Path) -> Option<FileStat> {
    let metadata = fs::metadata(path).ok()?;
    metadata.is_file().then(|| FileStat {
        len: metadata.len(),
        inode: metadata.ino(),
    })
}

/// Calls `on_line` with each complete line (without its `\n`) between
/// `*offset` and `len`, moving `*offset` past each line as it is handed over,
/// so an error part way leaves `*offset` at the first line not consumed. An
/// unfinished last line is left for a later read. `O_NONBLOCK` keeps a FIFO
/// put in the file's place after [`stat`] from blocking the open.
pub(super) fn read_lines(
    path: &Path,
    offset: &mut u64,
    len: u64,
    mut on_line: impl FnMut(&str),
) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    file.seek(SeekFrom::Start(*offset))?;
    let mut reader = BufReader::new(file.take(len.saturating_sub(*offset)));
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        let Some(body) = line.strip_suffix(b"\n") else {
            return Ok(());
        };
        *offset += read as u64;
        on_line(&String::from_utf8_lossy(body));
    }
}

#[cfg(test)]
#[path = "tail_tests.rs"]
mod tests;
