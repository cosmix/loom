//! Pure catalog ingestion over the knowledge tree.
//!
//! [`ingest`] reads `doc/loom/knowledge/**/*.md` and turns it into a
//! [`Catalog`] plus an [`IngestReport`] summarizing the build. It is pure and
//! side-effect free: it reads bytes and writes nothing.
//!
//! **HARD CONSTRAINT: `ingest` must not modify one byte of the knowledge
//! tree.** Duplicate headings, generic blurbs, and broken links are surfaced
//! through [`IngestReport::issues`] and never repaired here — repair, if it
//! ever exists, is a separate, explicit operation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::context::fingerprint::fingerprint_tree;
use crate::fs::knowledge::catalog::{self, Catalog, CatalogIssue};

/// Summary of one catalog build, for `loom knowledge status` / `sync` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestReport {
    /// Root of the knowledge tree that was ingested.
    pub knowledge_root: PathBuf,
    /// Number of `*.md` files fingerprinted.
    pub files: usize,
    /// Number of chunks the catalog was split into.
    pub chunks: usize,
    /// Catalog revision produced by this build.
    pub revision: String,
    /// Problems found in the knowledge base. REPORTED, never repaired.
    pub issues: Vec<CatalogIssue>,
}

/// Build a catalog from the knowledge tree. Pure and side-effect free: reads
/// bytes, writes nothing.
pub fn ingest(knowledge_root: &Path) -> Result<(Catalog, IngestReport)> {
    let catalog = catalog::build(knowledge_root)?;
    let fingerprints = fingerprint_tree(knowledge_root)?;

    let report = IngestReport {
        knowledge_root: knowledge_root.to_path_buf(),
        files: fingerprints.len(),
        chunks: catalog.chunks.len(),
        revision: catalog.revision.clone(),
        issues: catalog.issues.clone(),
    };

    Ok((catalog, report))
}
