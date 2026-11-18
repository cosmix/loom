//! Content fingerprints of the briefs the project holds no text for.
//!
//! A plan's body hash covers its brief bundle. Markdown briefs are hashed from their cached text;
//! other files (a `schema.json` beside the briefs) are read once here so an edit to them still
//! changes the hash.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::is_markdown;
use crate::context::source_graph::body_hash;
use crate::planning::discover::{read_doc_bytes, DocTree};
use crate::planning::kinds::DocKind;

/// A fingerprint per non-Markdown brief, by repo-relative path.
pub(super) type Fingerprints = BTreeMap<PathBuf, String>;

/// The fingerprints of `tree`'s non-Markdown briefs: `cached` entries are kept unless the
/// document went away or its path is in `stale`; the rest are read.
pub(super) fn collect(
    root: &Path,
    tree: &DocTree,
    mut cached: Fingerprints,
    stale: &BTreeSet<&Path>,
) -> Fingerprints {
    cached.retain(|path, _| tree.get(path).is_some() && !stale.contains(path.as_path()));
    let briefs = tree
        .docs
        .iter()
        .filter(|doc| matches!(doc.kind, DocKind::Brief { .. }) && !is_markdown(&doc.path));
    for doc in briefs {
        cached
            .entry(doc.path.clone())
            .or_insert_with(|| fingerprint(root, &doc.path));
    }
    cached
}

/// The content hash of the file, or the reason it could not be read.
fn fingerprint(root: &Path, rel: &Path) -> String {
    match read_doc_bytes(root, rel) {
        Ok(bytes) => body_hash(&bytes),
        Err(error) => format!("unreadable: {error:#}"),
    }
}
