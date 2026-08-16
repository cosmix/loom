//! Content fingerprinting for the knowledge tree.
//!
//! A [`FileFingerprint`] and the aggregate [`tree_revision`] it feeds are the
//! single source of truth for "has the knowledge tree changed" — see
//! [`crate::context::refresh`]. Both are deliberately mtime-free: fingerprints
//! are a pure function of file bytes, so copying or touching a file without
//! changing its contents never triggers a rebuild.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Content identity of one knowledge file. Deliberately carries NO mtime and no
/// timestamp: two clones of the same bytes must fingerprint identically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFingerprint {
    /// Path relative to the knowledge root, with `/` separators.
    pub path: PathBuf,
    /// `sha256:<hex>` over the file bytes.
    pub content_hash: String,
    /// File size in bytes.
    pub size: u64,
}

/// Fingerprint one file under `knowledge_root`.
///
/// `path` is relative to `knowledge_root`; the returned [`FileFingerprint::path`]
/// is that same relative path, unchanged.
pub fn fingerprint_file(knowledge_root: &Path, path: &Path) -> Result<FileFingerprint> {
    let full_path = knowledge_root.join(path);
    let bytes =
        fs::read(&full_path).with_context(|| format!("Failed to read {}", full_path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let content_hash = format!("sha256:{}", hex::encode(hasher.finalize()));

    Ok(FileFingerprint {
        path: path.to_path_buf(),
        content_hash,
        size: bytes.len() as u64,
    })
}

/// Fingerprint every `*.md` file under `knowledge_root`, recursively, sorted by
/// relative path.
///
/// Skips dotfiles and dot-directories at any depth. A missing `knowledge_root`
/// yields an empty `Vec`, not an error — a project without a knowledge tree yet
/// is a valid, unbuilt state, not a failure.
pub fn fingerprint_tree(knowledge_root: &Path) -> Result<Vec<FileFingerprint>> {
    if !knowledge_root.exists() {
        return Ok(Vec::new());
    }

    let mut relative_paths = Vec::new();
    collect_markdown_files(knowledge_root, knowledge_root, &mut relative_paths)?;
    relative_paths.sort();

    relative_paths
        .into_iter()
        .map(|relative| fingerprint_file(knowledge_root, &relative))
        .collect()
}

/// Recursively collect `*.md` files under `dir`, relative to `knowledge_root`,
/// skipping any dotfile or dot-directory.
fn collect_markdown_files(knowledge_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("Failed to read directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", dir.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to read file type for {}", path.display()))?;

        if file_type.is_dir() {
            collect_markdown_files(knowledge_root, &path, out)?;
        } else if file_type.is_file() && name.ends_with(".md") {
            let relative = path
                .strip_prefix(knowledge_root)
                .with_context(|| format!("Failed to relativize {}", path.display()))?;
            // Rejoin with `/` explicitly so `FileFingerprint::path` never carries
            // a platform-specific separator, matching its documented contract.
            let normalized: PathBuf = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
                .into();
            out.push(normalized);
        }
    }

    Ok(())
}

/// Hex sha256 over the sorted `"<relative-path>:<content_hash>"` lines, each
/// newline-terminated.
///
/// Returns a bare hex string with **no** `sha256:` prefix — this is a
/// *revision* over the whole tree, not a content hash of one file. The catalog
/// `revision` field uses the same convention.
pub fn tree_revision(fingerprints: &[FileFingerprint]) -> String {
    let mut lines: Vec<String> = fingerprints
        .iter()
        .map(|fp| format!("{}:{}", fp.path.display(), fp.content_hash))
        .collect();
    lines.sort();

    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}
