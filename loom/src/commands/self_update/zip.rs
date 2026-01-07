//! Zip extraction with security protections against zip slip and zip bomb attacks.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::client::{create_http_client, download_with_limit, validate_response_status};

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
pub(crate) fn validate_zip_entry(file: &zip::read::ZipFile) -> Result<()> {
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

/// Download and extract a zip file from a URL to a destination directory.
/// Includes comprehensive security checks for zip slip and zip bomb attacks.
pub(crate) fn download_and_extract_zip(url: &str, dest_dir: &Path) -> Result<()> {
    let client = create_http_client()?;
    let response = client.get(url).send().context("Failed to download zip")?;

    validate_response_status(&response, "Zip download failed")?;
    let bytes = download_with_limit(response, MAX_ZIP_SIZE, "Zip download")?;

    // Create temp file
    let temp_path = dest_dir.with_extension("zip.tmp");
    fs::write(&temp_path, &bytes).context("Failed to write temp zip")?;

    // Open and validate archive before any extraction
    let file = File::open(&temp_path).context("Failed to open temp zip")?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;

    // Pre-validate all entries before extraction (fail fast on malicious archives)
    let mut total_uncompressed_size: u64 = 0;
    for i in 0..archive.len() {
        let file = archive.by_index(i).context("Failed to read zip entry")?;

        // Validate against zip bombs
        validate_zip_entry(&file)?;

        // Track total size with overflow protection
        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| {
                anyhow::anyhow!("Total uncompressed size overflow - possible zip bomb")
            })?;

        if total_uncompressed_size > MAX_TOTAL_EXTRACTED_SIZE {
            bail!(
                "Total uncompressed size {total_uncompressed_size} exceeds maximum {MAX_TOTAL_EXTRACTED_SIZE} bytes - possible zip bomb"
            );
        }

        // Validate path safety (using enclosed_name for additional safety, falling back to mangled_name)
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file.mangled_name().to_string_lossy().to_string());

        // Skip empty names (root directory entries)
        if !entry_name.is_empty() {
            safe_extract_path(dest_dir, &entry_name)?;
        }
    }

    // Backup existing directory (only after validation passes)
    if dest_dir.exists() {
        let backup = dest_dir.with_extension("bak");
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(dest_dir, &backup).context("Failed to backup directory")?;
    }

    // Re-open archive for extraction (we consumed it during validation)
    let file = File::open(&temp_path).context("Failed to reopen temp zip")?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to reread zip archive")?;

    fs::create_dir_all(dest_dir)?;

    // Extract with validated paths
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // Get safe entry name (prefer enclosed_name, fall back to mangled_name)
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file.mangled_name().to_string_lossy().to_string());

        // Skip empty names
        if entry_name.is_empty() {
            continue;
        }

        // Get safe output path (already validated, but re-verify for defense in depth)
        let outpath = safe_extract_path(dest_dir, &entry_name)?;

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }

            // Extract with size limit enforcement during decompression
            let mut outfile = File::create(&outpath)
                .with_context(|| format!("Failed to create file: {}", outpath.display()))?;

            // Use a limited reader to enforce size during extraction
            // This catches zip bombs that lie about their uncompressed size in headers
            let mut limited_reader = LimitedReader::new(&mut file, MAX_UNCOMPRESSED_SIZE);
            io::copy(&mut limited_reader, &mut outfile)
                .with_context(|| format!("Failed to extract file: {entry_name}"))?;
        }
    }

    // Cleanup
    fs::remove_file(&temp_path).ok();
    let backup = dest_dir.with_extension("bak");
    if backup.exists() {
        fs::remove_dir_all(&backup).ok();
    }

    Ok(())
}
