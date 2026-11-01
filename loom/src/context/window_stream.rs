//! Stream the complete identity while retaining only the selected whole lines.
//! Selected lines have no byte cap: even one very long line is returned whole.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::Context;
use sha2::{Digest, Sha256};

use super::{SourceWindow, Span, Target};
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};

pub(super) enum ReadError {
    Changed,
    Io(anyhow::Error),
}

impl From<anyhow::Error> for ReadError {
    fn from(error: anyhow::Error) -> Self {
        Self::Io(error)
    }
}

pub(super) fn read(
    root: &Path,
    target: &Target,
    max_lines: usize,
    limit: usize,
    expected: &str,
) -> Result<SourceWindow, ReadError> {
    let root_fd = safe_open_dirfd(root)?;
    let fd = open_safely(
        root_fd.as_raw_fd(),
        Path::new(&target.path),
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )?;
    let mut file = File::from(fd);
    let metadata = file.metadata().context("inspecting source")?;
    if !metadata.is_file() {
        return Err(ReadError::Io(anyhow::anyhow!(
            "opened path is not a regular file"
        )));
    }
    if metadata.len() > limit as u64 {
        return Err(ReadError::Changed);
    }
    let size = metadata.len() as usize;
    let start = target.start.min(size);
    let end = target.end.clamp(start, size);
    let (begin, first_hash, first_size) = line_begin(&mut file, start, limit)?;
    if first_size != size || first_hash != expected {
        return Err(ReadError::Changed);
    }
    let last = end.saturating_sub(1).max(start);
    let (region, line_start, truncated, hash, actual) =
        collect(&mut file, begin, last, max_lines, limit)?;
    if actual != size || hash != expected {
        return Err(ReadError::Changed);
    }
    Ok(build_window(target, begin, region, line_start, truncated))
}

fn build_window(
    target: &Target,
    begin: usize,
    region: Vec<u8>,
    line_start: usize,
    truncated: bool,
) -> SourceWindow {
    let newlines = region.iter().filter(|&&b| b == b'\n').count();
    let lines = newlines + usize::from(!region.is_empty() && !region.ends_with(b"\n"));
    SourceWindow {
        path: target.path.clone(),
        span: Span {
            start_byte: begin,
            end_byte: begin + region.len(),
            line_start,
            line_end: line_start + lines.saturating_sub(1),
        },
        text: String::from_utf8_lossy(&region).into_owned(),
        truncated,
    }
}

/// Find the line boundary in an authenticated pass with fixed storage.
fn line_begin(
    file: &mut File,
    start: usize,
    limit: usize,
) -> Result<(usize, String, usize), ReadError> {
    let mut offset = 0usize;
    let mut begin = 0usize;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let count = match file.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.context("reading source")?,
        };
        if count == 0 {
            break;
        }
        if count > limit.saturating_sub(offset) {
            return Err(ReadError::Changed);
        }
        hash.update(&buffer[..count]);
        let prefix = start.saturating_sub(offset).min(count);
        if let Some(index) = buffer[..prefix].iter().rposition(|&b| b == b'\n') {
            begin = offset + index + 1;
        }
        offset += count;
    }
    Ok((
        begin,
        format!("sha256:{}", hex::encode(hash.finalize())),
        offset,
    ))
}

fn collect(
    file: &mut File,
    begin: usize,
    last: usize,
    max_lines: usize,
    limit: usize,
) -> Result<(Vec<u8>, usize, bool, String, usize), ReadError> {
    file.seek(SeekFrom::Start(0)).context("seeking source")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    let mut selection = Selection {
        begin,
        last,
        max_lines,
        region: Vec::new(),
        line_start: 1,
        lines: 0,
        done: false,
        truncated: false,
    };
    let mut offset = 0usize;
    loop {
        let count = match file.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.context("reading source")?,
        };
        if count == 0 {
            break;
        }
        if count > limit.saturating_sub(offset) {
            return Err(ReadError::Changed);
        }
        hash.update(&buffer[..count]);
        selection.accept(&buffer[..count], offset);
        offset += count;
    }
    if begin > offset {
        return Err(ReadError::Changed);
    }
    Ok((
        selection.region,
        selection.line_start,
        selection.truncated,
        format!("sha256:{}", hex::encode(hash.finalize())),
        offset,
    ))
}

struct Selection {
    begin: usize,
    last: usize,
    max_lines: usize,
    region: Vec<u8>,
    line_start: usize,
    lines: usize,
    done: bool,
    truncated: bool,
}

impl Selection {
    fn accept(&mut self, chunk: &[u8], offset: usize) {
        if self.done {
            return;
        }
        for (index, &byte) in chunk.iter().enumerate() {
            let pos = offset + index;
            if pos < self.begin {
                self.line_start += usize::from(byte == b'\n');
                continue;
            }
            if self.lines < self.max_lines {
                self.region.push(byte);
            } else {
                self.truncated = true;
            }
            if byte == b'\n' {
                self.lines += 1;
                if pos >= self.last {
                    self.done = true;
                    break;
                }
            }
        }
    }
}
