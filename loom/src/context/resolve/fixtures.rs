//! Hand-built graph fixtures shared by the resolution and traversal tests.
//!
//! Every graph in these tests is written out by hand: the point of the tests is
//! the honesty contract, so what evidence exists must be visible in the fixture
//! rather than produced by a real extractor run.

use std::path::{Path, PathBuf};

use crate::context::graph_store::{FileEntry, ResolvedGraph};
use crate::context::source_graph::{
    node_id, EdgeProvenance, FileCoverage, NodeLanguage, SourceEdge, SourceEdgeKind, SourceNode,
    SourceNodeKind, Span,
};

pub(crate) fn file_node(path: &str) -> SourceNode {
    SourceNode {
        id: path.to_string(),
        kind: SourceNodeKind::File,
        path: PathBuf::from(path),
        scope: Vec::new(),
        span: Span::default(),
        signature: String::new(),
        body_hash: "sha256:node".to_string(),
        language: NodeLanguage::Rust,
        parser_version: "test".to_string(),
        coverage: FileCoverage::Full,
    }
}

/// Canonical id of a one-segment symbol, matching what the extractors emit.
pub(crate) fn scoped_id(path: &str, kind: SourceNodeKind, name: &str) -> String {
    node_id(Path::new(path), kind, &[name.to_string()])
}

pub(crate) fn func_id(path: &str, name: &str) -> String {
    scoped_id(path, SourceNodeKind::Function, name)
}

pub(crate) fn scoped_node(path: &str, kind: SourceNodeKind, name: &str) -> SourceNode {
    SourceNode {
        id: scoped_id(path, kind, name),
        kind,
        scope: vec![name.to_string()],
        signature: format!("{kind} {name}"),
        ..file_node(path)
    }
}

/// A file entry whose symbol nodes are given as `(kind, name)` pairs — for the
/// cases where the kind is the point, such as a type beside its `impl` blocks.
pub(crate) fn mixed_file(
    path: &str,
    symbols: &[(SourceNodeKind, &str)],
    edges: Vec<SourceEdge>,
) -> FileEntry {
    let mut nodes = vec![file_node(path)];
    nodes.extend(
        symbols
            .iter()
            .map(|(kind, name)| scoped_node(path, *kind, name)),
    );
    FileEntry {
        content_hash: "sha256:file".to_string(),
        nodes,
        edges,
        coverage: FileCoverage::Full,
    }
}

/// A file entry holding a file node plus one function node per name.
pub(crate) fn source_file(path: &str, symbols: &[&str], edges: Vec<SourceEdge>) -> FileEntry {
    let functions: Vec<(SourceNodeKind, &str)> = symbols
        .iter()
        .map(|name| (SourceNodeKind::Function, *name))
        .collect();
    mixed_file(path, &functions, edges)
}

pub(crate) fn graph_of(files: Vec<(&str, FileEntry)>) -> ResolvedGraph {
    ResolvedGraph {
        files: files
            .into_iter()
            .map(|(path, entry)| (path.to_string(), entry))
            .collect(),
        ..ResolvedGraph::default()
    }
}

/// A graph of function-bearing files: `(path, function names, edges)`.
pub(crate) fn graph_from(files: Vec<(&str, &[&str], Vec<SourceEdge>)>) -> ResolvedGraph {
    graph_of(
        files
            .into_iter()
            .map(|(path, symbols, edges)| (path, source_file(path, symbols, edges)))
            .collect(),
    )
}

/// An edge with fields set exactly as written — the only way to build one the
/// extractor constructors would refuse (a `Parser` edge left unresolved, or an
/// inferred edge above the extraction ceiling).
pub(crate) fn edge_at(
    from: &str,
    to: &str,
    kind: SourceEdgeKind,
    provenance: EdgeProvenance,
    confidence: f32,
) -> SourceEdge {
    SourceEdge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        provenance,
        confidence,
        symbol: String::new(),
    }
}
