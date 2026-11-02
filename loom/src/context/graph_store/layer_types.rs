//! The persisted shapes of the layered source graph: [`FileEntry`],
//! [`GraphLayer`] and the [`ResolvedGraph`] view a reader sees.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::context::extract::FileExtraction;
use crate::context::source_graph::{
    body_hash, FileCoverage, ImportBinding, SourceEdge, SourceNode, GRAPH_SCHEMA_VERSION,
};

/// One file's contribution to a layer.
///
/// Stored per-file rather than as two flat lists so an overlay can shadow
/// exactly the files a stage touched, and so a single changed file can be
/// re-extracted without rebuilding the layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    /// `sha256:<hex>` over the file's exact bytes, for incremental rebuilds.
    pub content_hash: String,
    #[serde(default)]
    pub nodes: Vec<SourceNode>,
    #[serde(default)]
    pub edges: Vec<SourceEdge>,
    pub coverage: FileCoverage,
    /// Import bindings the file declares, for cross-file resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<ImportBinding>,
}

impl Default for FileEntry {
    /// An entry nothing has been extracted into yet. The coverage says so
    /// rather than claiming `Full`, so a half-built layer can never read as
    /// complete.
    fn default() -> Self {
        FileEntry {
            content_hash: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            coverage: FileCoverage::LexicalOnly {
                detail: "not extracted".to_string(),
            },
            imports: Vec::new(),
        }
    }
}

impl FileEntry {
    /// A deletion marker carried only by an overlay.
    pub fn tombstone() -> Self {
        Self {
            content_hash: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            coverage: FileCoverage::Deleted,
            imports: Vec::new(),
        }
    }

    /// The entry for a file whose `bytes` produced `extraction`.
    pub fn from_extraction(bytes: &[u8], extraction: FileExtraction) -> Self {
        Self {
            content_hash: body_hash(bytes),
            nodes: extraction.nodes,
            edges: extraction.edges,
            coverage: extraction.coverage,
            imports: extraction.imports,
        }
    }
}

/// One persisted layer: base or overlay.
///
/// `files` is a `BTreeMap` and every collection inside is sorted, so two runs
/// over identical bytes serialize byte-identically (see `canonical_json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphLayer {
    /// Source revision this layer describes: a git commit for a base layer,
    /// and the base revision the overlay was cut from for an overlay.
    #[serde(default)]
    pub revision: String,
    /// Snapshot generation identifier; empty means unknown until
    /// `source-graph-snapshot` fills it.
    #[serde(default)]
    pub generation: String,
    /// When this layer was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<DateTime<Utc>>,
    /// Extraction results keyed by project-relative, forward-slashed path.
    #[serde(default)]
    pub files: BTreeMap<String, FileEntry>,
    /// Path to the git blob object id whose bytes produced its
    /// [`FileEntry::content_hash`].
    #[serde(default)]
    pub blob_index: BTreeMap<String, String>,
    /// [`GRAPH_SCHEMA_VERSION`](crate::context::source_graph::GRAPH_SCHEMA_VERSION) the layer was written under; a layer written
    /// before schema versioning deserializes as `0`.
    #[serde(default)]
    pub schema_version: u32,
    /// An overlay built with no base at `revision`: `files` holds the whole
    /// tree, so nothing belongs under it even if that base is published later.
    /// False for a delta, whose files mean nothing without that base.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub complete: bool,
}

impl GraphLayer {
    /// Every node in this layer, in path order.
    pub fn nodes(&self) -> impl Iterator<Item = &SourceNode> {
        self.files.values().flat_map(|entry| entry.nodes.iter())
    }

    /// Every edge in this layer, in path order.
    pub fn edges(&self) -> impl Iterator<Item = &SourceEdge> {
        self.files.values().flat_map(|entry| entry.edges.iter())
    }

    /// True when the layer was written under the current [`GRAPH_SCHEMA_VERSION`];
    /// any other layer is never served or reused.
    pub(crate) fn has_current_schema(&self) -> bool {
        self.schema_version == GRAPH_SCHEMA_VERSION
    }
}

/// A base layer with an overlay applied — the view every reader sees.
///
/// Built by `GraphStore::resolved`. Holds owned data because the two layers
/// it draws from have different lifetimes and a reader should not have to care
/// which layer an entry came from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolvedGraph {
    /// Revision of the base layer underneath, or of a complete overlay that
    /// needs none; empty when neither supplied it.
    pub base_revision: String,
    /// Paths the overlay shadowed. Non-empty means this view is stage-local.
    pub overlaid: BTreeSet<String>,
    pub files: BTreeMap<String, FileEntry>,
}

impl ResolvedGraph {
    pub fn nodes(&self) -> impl Iterator<Item = &SourceNode> {
        self.files.values().flat_map(|entry| entry.nodes.iter())
    }

    pub fn edges(&self) -> impl Iterator<Item = &SourceEdge> {
        self.files.values().flat_map(|entry| entry.edges.iter())
    }

    /// Look up a node by its [`SourceNode::id`].
    pub fn node(&self, id: &str) -> Option<&SourceNode> {
        self.nodes().find(|node| node.id == id)
    }

    /// Total node count, for coverage reporting.
    pub fn node_count(&self) -> usize {
        self.files.values().map(|entry| entry.nodes.len()).sum()
    }

    /// Total edge count, for coverage reporting.
    pub fn edge_count(&self) -> usize {
        self.files.values().map(|entry| entry.edges.len()).sum()
    }
}
