//! Zip extraction with security protections against zip slip and zip bomb attacks.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

// Zip Extraction Security Constants
/// Maximum uncompressed size for any single zip entry (100 MB)
pub(crate) const MAX_UNCOMPRESSED_SIZE: u64 = 100 * 1024 * 1024;
/// Maximum compression ratio to detect zip bombs (normal files rarely exceed 20:1)
pub(crate) const MAX_COMPRESSION_RATIO: f64 = 100.0;
/// Maximum total extracted size for all entries combined (500 MB)
pub(crate) const MAX_TOTAL_EXTRACTED_SIZE: u64 = 500 * 1024 * 1024;
/// Maximum size for zip archives
pub(crate) const MAX_ZIP_SIZE: u64 = 100 * 1024 * 1024; // 100MB for zip archives

/// A reader wrapper that limits the number of bytes that can be read.
/// Used to prevent zip bombs that lie about their uncompressed size in headers.
pub(crate) struct LimitedReader<R> {
    inner: R,
    remaining: u64,
}

impl<R> LimitedReader<R> {
    pub(crate) fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other(
                "Zip entry exceeds maximum allowed size during extraction - possible zip bomb",
            ));
        }

        // Limit the read to remaining bytes
        let max_read = std::cmp::min(buf.len() as u64, self.remaining) as usize;
        let bytes_read = self.inner.read(&mut buf[..max_read])?;
        self.remaining = self.remaining.saturating_sub(bytes_read as u64);

        Ok(bytes_read)
    }
}

/// Validates a zip entry for security threats (zip bombs, excessive size).
///
/// Checks:
/// - Absolute uncompressed size against MAX_UNCOMPRESSED_SIZE
/// - Compression ratio against MAX_COMPRESSION_RATIO to detect zip bombs
pub(crate) fn validate_zip_entry<R: Read + ?Sized>(file: &zip::read::ZipFile<'_, R>) -> Result<()> {
    let compressed = file.compressed_size();
    let uncompressed = file.size();

    // Check absolute size limit
    if uncompressed > MAX_UNCOMPRESSED_SIZE {
        bail!(
            "Zip entry '{}' too large: {} bytes (max: {} bytes)",
            file.name(),
            uncompressed,
            MAX_UNCOMPRESSED_SIZE
        );
    }

    // Check compression ratio for zip bomb detection
    if compressed > 0 {
        let ratio = uncompressed as f64 / compressed as f64;
        if ratio > MAX_COMPRESSION_RATIO {
            bail!(
                "Suspicious compression ratio in '{}': {:.1}x (max: {:.1}x) - possible zip bomb",
                file.name(),
                ratio,
                MAX_COMPRESSION_RATIO
            );
        }
    }

    Ok(())
}

/// Safely resolves a zip entry path, protecting against zip slip attacks.
///
/// Returns the safe output path if the entry is valid, or an error if:
/// - The path contains ".." components (directory traversal attempt)
/// - The resolved path escapes the destination directory
/// - The path is absolute (would ignore dest_dir)
pub(crate) fn safe_extract_path(dest_dir: &Path, entry_name: &str) -> Result<PathBuf> {
    // Reject paths containing ".." anywhere - explicit directory traversal attempt
    if entry_name.contains("..") {
        bail!("Zip slip attack detected: path contains '..' component - '{entry_name}'");
    }

    // Reject absolute paths that would ignore dest_dir
    let entry_path = Path::new(entry_name);
    if entry_path.is_absolute() {
        bail!("Zip slip attack detected: absolute path in archive - '{entry_name}'");
    }

    // Reject paths starting with / or \ (platform-specific absolute indicators)
    if entry_name.starts_with('/') || entry_name.starts_with('\\') {
        bail!("Zip slip attack detected: path starts with path separator - '{entry_name}'");
    }

    // Build the output path
    // Canonicalize dest_dir to resolve any symlinks in the destination
    // We need to ensure dest_dir exists first for canonicalize to work
    fs::create_dir_all(dest_dir).context("Failed to create destination directory")?;
    let canonical_dest = dest_dir
        .canonicalize()
        .context("Failed to canonicalize destination directory")?;

    // For the output path, we check component by component since it may not exist yet
    // Normalize the path by resolving . and removing redundant separators
    let mut normalized = canonical_dest.clone();
    for component in entry_path.components() {
        use std::path::Component;
        match component {
            Component::Normal(c) => normalized.push(c),
            Component::CurDir => {} // Skip "."
            Component::ParentDir => {
                // This shouldn't happen since we checked for ".." above, but be defensive
                bail!("Zip slip attack detected: parent directory traversal in '{entry_name}'");
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("Zip slip attack detected: absolute path component in '{entry_name}'");
            }
        }
    }

    // Final verification: the normalized path must start with the canonical destination
    if !normalized.starts_with(&canonical_dest) {
        bail!(
            "Zip slip attack detected: resolved path '{}' escapes destination directory '{}'",
            normalized.display(),
            canonical_dest.display()
        );
    }

    Ok(normalized)
}
