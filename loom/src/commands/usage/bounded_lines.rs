use std::fs::{File, OpenOptions};
use std::io::{self, BufReader};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::tokens::tail::{next_line, MAX_LINE_BYTES};

pub(super) fn open_regular(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(file)
}

pub(super) fn for_each_line(
    file: File,
    mut on_line: impl FnMut(usize, Option<&str>),
) -> io::Result<()> {
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut ordinal = 0;
    loop {
        let next = next_line(&mut reader, &mut line, MAX_LINE_BYTES)?;
        let cut = match next {
            Some((_, cut)) => cut,
            None if line.is_empty() => break,
            // At EOF, next_line leaves a prefix but doesn't report whether it was cut.
            None => line.len() == MAX_LINE_BYTES,
        };
        if next.is_some() && line.last() == Some(&b'\r') {
            line.pop();
        }
        let text = if cut {
            None
        } else {
            std::str::from_utf8(&line).ok()
        };
        on_line(ordinal, text);
        if next.is_none() {
            break;
        }
        ordinal += 1;
    }
    Ok(())
}

#[cfg(test)]
#[path = "bounded_lines_tests.rs"]
mod tests;
