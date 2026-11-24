//! Catalog build that also reports the fingerprints of the bytes it indexed.

use super::{build_curated, display_path, markdown_files, prose, revision_for, Catalog};
use crate::context::fingerprint::{fingerprint_bytes, FileFingerprint};
use crate::fs::knowledge::read::read_knowledge_file;
use crate::fs::knowledge::types::INDEX_FILENAME;
use anyhow::Context;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Fingerprint of the root `INDEX.md`, or `None` when there is none to count.
///
/// Only a regular file counts. A symlink, FIFO or directory at that name is
/// skipped silently, as the catalog walker skips such entries elsewhere, so
/// the catalog build and [`curated_tree_fingerprints`] agree on the file set.
/// A nested `INDEX.md` is never counted: `markdown_files` skips every one.
pub(crate) fn root_index_fingerprint(root: &Path) -> anyhow::Result<Option<FileFingerprint>> {
    let index = Path::new(INDEX_FILENAME);
    match fs::symlink_metadata(root.join(index)) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Ok(None),
        // A knowledge root that is a regular file holds no INDEX.md, as
        // `markdown_files` finds no files under it.
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(None)
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect {}", root.join(index).display()))
        }
    }
    match read_knowledge_file(root, index) {
        Ok(bytes) => Ok(Some(fingerprint_bytes(index.to_path_buf(), &bytes))),
        Err(error) if crate::fs::safe_read::is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Fingerprints of the curated tree: every file the catalog walker indexes
/// plus the root `INDEX.md`, sorted by relative path. The same set
/// [`build_with_fingerprints`] reports, hashed without building the catalog.
pub(crate) fn curated_tree_fingerprints(root: &Path) -> anyhow::Result<Vec<FileFingerprint>> {
    let mut fingerprints = markdown_files(root)?
        .into_iter()
        .map(|relative| {
            let bytes = read_knowledge_file(root, &relative)?;
            Ok(fingerprint_bytes(
                PathBuf::from(display_path(&relative)),
                &bytes,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    fingerprints.extend(root_index_fingerprint(root)?);
    fingerprints.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(fingerprints)
}

/// Build the catalog and fingerprint every file it read, from the same bytes.
///
/// The fingerprint list matches `fingerprint_tree` file for file (curated
/// files, the root `INDEX.md`, indexed prose), so its `tree_revision` describes exactly
/// what the catalog indexed: an edit between a freshness pre-pass and this
/// build cannot leave new chunks under an old revision.
///
/// Prose is appended to the SAME chunk list the curated tree produced, so one
/// BM25 corpus covers both and the catalog revision folds prose edits in.
/// Prose contributes no `issues`: the duplicate-heading, generic-blurb,
/// broken-link and missing-source-ref diagnostics are contracts on the CURATED
/// tree, and reporting them for arbitrary project docs would bury the ones an
/// author can act on.
pub(crate) fn build_with_fingerprints(
    root: &Path,
) -> anyhow::Result<(Catalog, Vec<FileFingerprint>)> {
    let (mut chunks, issues, mut fingerprints) = build_curated(root, true)?;

    // The catalog skips `INDEX.md`; the tree fingerprint hashes the root one.
    fingerprints.extend(root_index_fingerprint(root)?);
    if let Some(sources) = prose::sources_for_knowledge_root(root) {
        let (prose_chunks, prose_prints) = sources.chunks_and_fingerprints();
        chunks.extend(prose_chunks);
        fingerprints.extend(prose_prints);
    }
    // A missing root has no structural layer, whatever prose exists.
    if !root.exists() {
        fingerprints.clear();
    }

    let catalog = Catalog {
        revision: revision_for(&chunks),
        chunks,
        issues,
    };
    Ok((catalog, fingerprints))
}
