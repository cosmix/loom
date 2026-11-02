//! Bounded tail reads on verified regular file descriptors.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::Result;

use super::{open_safely, regular_file_metadata, safe_open_dirfd};

/// Read a tail even when the whole file exceeds the cap. Growth after the
/// size snapshot stays bounded by `max_bytes`.
pub(crate) fn read_tail(root: &Path, relative: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let root_fd = safe_open_dirfd(root)?;
    let fd = open_safely(
        root_fd.as_raw_fd(),
        relative,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )?;
    let mut file = File::from(fd);
    let len = regular_file_metadata(&file)?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(max_bytes)))?;
    read_capped(file, max_bytes)
}

fn read_capped(source: impl Read, max_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    source.take(max_bytes).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_continuously_growing_source_is_capped() {
        let bytes = super::read_capped(std::io::repeat(b'x'), 123).unwrap();
        assert_eq!(bytes, vec![b'x'; 123]);
    }
}
