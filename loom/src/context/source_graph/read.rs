//! One bounded source reader for refresh and verification graphs.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::Path;

use super::{FileCoverage, MAX_EXTRACTED_FILE_BYTES};
use crate::context::extract::{extract_file, file_node, lexical};
use crate::context::graph_store::FileEntry;
use crate::context::refresh::BoxedExtractor;
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};

/// Full content identity with only the extractable prefix retained.
#[derive(Debug)]
pub(crate) struct SourceRead {
    pub bytes: Vec<u8>,
    pub size: usize,
    pub hash: String,
    lines: usize,
}

impl SourceRead {
    /// Hash all bytes through a fixed buffer; never reserve from a file's size.
    pub(crate) fn read(mut reader: impl Read) -> Result<Self> {
        let mut bytes = Vec::new();
        let mut size = 0usize;
        let mut lines = 0usize;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let count = match reader.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result.context("reading source bytes")?,
            };
            if count == 0 {
                break;
            }
            let chunk = &buffer[..count];
            hash.update(chunk);
            size = size.checked_add(count).context("source size overflow")?;
            lines += chunk.iter().filter(|&&byte| byte == b'\n').count();
            let retained = count.min(MAX_EXTRACTED_FILE_BYTES - bytes.len());
            bytes.extend_from_slice(&chunk[..retained]);
        }
        Ok(Self {
            bytes,
            size,
            hash: format!("sha256:{}", hex::encode(hash.finalize())),
            lines,
        })
    }

    pub(crate) fn entry(self, extractors: &[BoxedExtractor], path: &Path) -> FileEntry {
        if self.size <= MAX_EXTRACTED_FILE_BYTES {
            return FileEntry::from_extraction(
                &self.bytes,
                extract_file(extractors, path, &self.bytes),
            );
        }
        let coverage = FileCoverage::Oversized {
            bytes: self.size,
            limit: MAX_EXTRACTED_FILE_BYTES,
        };
        let mut node = file_node(
            path,
            &[],
            lexical::language_for_path(path),
            lexical::LEXICAL_PARSER_VERSION.to_string(),
            &coverage,
        );
        node.body_hash = self.hash.clone();
        node.span.end_byte = self.size;
        node.span.line_end = self.lines.max(1);
        FileEntry {
            content_hash: self.hash,
            nodes: vec![node],
            edges: Vec::new(),
            coverage,
            imports: Vec::new(),
        }
    }
}

/// Open each component without following links, then stream the regular file.
pub(crate) fn read_source(root: &Path, relative: &Path) -> Result<SourceRead> {
    let root_fd = safe_open_dirfd(root)?;
    let fd = open_safely(
        root_fd.as_raw_fd(),
        relative,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        bail!("source is not a regular file: {}", relative.display());
    }
    SourceRead::read(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::source_graph::body_hash;

    #[test]
    fn preserves_arbitrary_bytes_and_whitespace() {
        let bytes = b"\n\nfn original() {}\n\xff \t\n";
        let read = SourceRead::read(&bytes[..]).unwrap();
        assert_eq!(read.bytes, bytes);
        assert_eq!(read.hash, body_hash(bytes));
    }

    #[test]
    fn sparse_oversized_source_has_bounded_retention_and_full_identity() {
        use std::io::{Seek, SeekFrom, Write};
        let dir = tempfile::tempdir().unwrap();
        let file = File::create(dir.path().join("large.rs")).unwrap();
        let size = MAX_EXTRACTED_FILE_BYTES * 32;
        file.set_len(size as u64).unwrap();
        let first = read_source(dir.path(), Path::new("large.rs")).unwrap();
        let second = read_source(dir.path(), Path::new("large.rs")).unwrap();
        assert_eq!(first.bytes.len(), MAX_EXTRACTED_FILE_BYTES);
        assert!(first.bytes.capacity() <= MAX_EXTRACTED_FILE_BYTES);
        assert_eq!(first.hash, second.hash);
        assert_eq!(first.size, size);
        let entry = first.entry(&crate::context::extract::registry(), Path::new("large.rs"));
        assert_eq!(entry.content_hash, second.hash);
        assert_eq!(entry.nodes[0].span.end_byte, size);
        assert!(matches!(entry.coverage, FileCoverage::Oversized { bytes, .. } if bytes == size));
        let mut tail = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join("large.rs"))
            .unwrap();
        tail.seek(SeekFrom::End(-1)).unwrap();
        tail.write_all(b"x").unwrap();
        let changed = read_source(dir.path(), Path::new("large.rs")).unwrap();
        assert_eq!(changed.bytes, second.bytes);
        assert_ne!(
            changed.hash, second.hash,
            "the hash must include bytes beyond retention"
        );
    }

    #[test]
    fn interrupted_reads_retry() {
        struct InterruptedOnce(bool);
        impl Read for InterruptedOnce {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                (&b""[..]).read(buffer)
            }
        }
        assert_eq!(SourceRead::read(InterruptedOnce(false)).unwrap().size, 0);
    }
}
