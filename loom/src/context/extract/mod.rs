//! Per-language source extraction: bytes in, [`FileExtraction`] out.
//!
//! Each supported language implements [`SourceGraphExtractor`] over a pinned
//! tree-sitter grammar and a tree-sitter query embedded in that language's
//! module. The registry in this module is the only thing the rest of loom sees;
//! callers never name a grammar directly.
//!
//! ## What extraction promises, and what it does not
//!
//! An extractor promises that every node it emits corresponds to a real
//! declaration in the bytes it was handed, and that every edge it emits carries
//! honest [`crate::context::source_graph::EdgeProvenance`]. It does **not**
//! promise a complete call graph: extraction is per-file, so a call to a symbol
//! defined in another file is emitted as an inferred edge (or as unresolved),
//! never as a parser edge. Cross-file resolution is `crate::context::resolve`'s
//! job, and it too only ever raises confidence with evidence.
//!
//! ## Degraded modes
//!
//! Nothing makes a file vanish from the graph:
//!
//! | Situation                              | Result                                      |
//! | -------------------------------------- | ------------------------------------------- |
//! | No grammar for the language            | file node, `FileCoverage::LexicalOnly`      |
//! | File over [`MAX_EXTRACTED_FILE_BYTES`] | file node, `FileCoverage::Oversized`        |
//! | Grammar reports a syntax error         | file node, `FileCoverage::ParseError`       |
//! | `source-graph` cargo feature disabled  | file node, `FileCoverage::LexicalOnly`      |

use anyhow::Result;
use std::path::Path;

use crate::context::source_graph::{
    body_hash, file_node_id, FileCoverage, NodeLanguage, SourceEdge, SourceNode, SourceNodeKind,
    Span, MAX_EXTRACTED_FILE_BYTES,
};
use crate::language::DetectedLanguage;

pub mod lexical;

#[cfg(feature = "source-graph")]
pub mod go;
#[cfg(feature = "source-graph")]
pub mod python;
#[cfg(feature = "source-graph")]
pub mod rust;
#[cfg(feature = "source-graph")]
pub mod typescript;

#[cfg(feature = "source-graph")]
mod treesitter;

#[cfg(feature = "source-graph")]
pub use treesitter::{run_query, QueryHarness};

/// Identity of an extractor build.
///
/// Any change to the pinned grammar, the embedded query, or the walking logic
/// must change this, or a cached extraction from an older build will be
/// silently reused. `query_digest` is a hash rather than the query text so the
/// identity stays small enough to store on every node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractorIdentity {
    /// Version of the pinned tree-sitter grammar crate.
    pub grammar_version: &'static str,
    /// `sha256:<hex>` over the embedded query source.
    pub query_digest: String,
    /// Bumped by hand whenever the walking logic changes shape.
    pub extractor_version: u32,
}

impl ExtractorIdentity {
    /// Compact stable rendering, stored as [`SourceNode::parser_version`].
    pub fn to_parser_version(&self) -> String {
        // Only the first 12 hex digits of the digest: enough to separate query
        // revisions, short enough to repeat on every node without bloating the
        // cache.
        let digest = self
            .query_digest
            .strip_prefix("sha256:")
            .unwrap_or(&self.query_digest);
        let short: String = digest.chars().take(12).collect();
        format!(
            "{}+{}+v{}",
            self.grammar_version, short, self.extractor_version
        )
    }
}

/// Everything one file contributed to the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct FileExtraction {
    pub nodes: Vec<SourceNode>,
    pub edges: Vec<SourceEdge>,
    pub coverage: FileCoverage,
}

impl FileExtraction {
    /// A file-level-only extraction: one node, no edges.
    ///
    /// The single shared construction path for every degraded mode, so an
    /// unsupported language, an oversized file, and a parse error all keep
    /// file-level metadata in exactly the same shape.
    pub fn file_level(
        path: &Path,
        bytes: &[u8],
        language: NodeLanguage,
        parser_version: String,
        coverage: FileCoverage,
    ) -> Self {
        FileExtraction {
            nodes: vec![file_node(path, bytes, language, parser_version, &coverage)],
            edges: Vec::new(),
            coverage,
        }
    }
}

/// Build the whole-file node that every extraction carries.
pub fn file_node(
    path: &Path,
    bytes: &[u8],
    language: NodeLanguage,
    parser_version: String,
    coverage: &FileCoverage,
) -> SourceNode {
    SourceNode {
        id: file_node_id(path),
        kind: SourceNodeKind::File,
        path: path.to_path_buf(),
        scope: Vec::new(),
        span: whole_file_span(bytes),
        signature: String::new(),
        body_hash: body_hash(bytes),
        language,
        parser_version,
        coverage: coverage.clone(),
    }
}

/// Span covering an entire buffer.
pub fn whole_file_span(bytes: &[u8]) -> Span {
    let lines = bytes.iter().filter(|byte| **byte == b'\n').count();
    Span {
        start_byte: 0,
        end_byte: bytes.len(),
        line_start: 1,
        // A file with no trailing newline still ends on the line after the last
        // break; an empty file is one (empty) line.
        line_end: lines.max(1),
    }
}

/// One language's extraction strategy.
pub trait SourceGraphExtractor {
    /// The single language this extractor handles.
    fn language(&self) -> DetectedLanguage;

    /// Identity of this extractor build, for cache invalidation.
    fn cache_identity(&self) -> ExtractorIdentity;

    /// Whether this extractor claims `path` by its extension.
    fn supports(&self, path: &Path) -> bool;

    /// Extract nodes and edges from `bytes`, which must be the exact contents
    /// of `path`.
    ///
    /// `path` is used for ids and must be relative to the project root.
    /// Implementations return `Err` only for a genuine internal failure —
    /// a syntax error in the input is data, reported as
    /// [`FileCoverage::ParseError`], not an error.
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}

/// Every compiled-in extractor, in registration order.
///
/// Boxed rather than an enum so a host without the `source-graph` feature gets
/// an empty registry and the callers above it need no `cfg`.
pub fn registry() -> Vec<Box<dyn SourceGraphExtractor + Send + Sync>> {
    #[cfg(feature = "source-graph")]
    {
        vec![
            Box::new(rust::RustExtractor::new()),
            Box::new(typescript::TypeScriptExtractor::new()),
            Box::new(python::PythonExtractor::new()),
            Box::new(go::GoExtractor::new()),
        ]
    }
    #[cfg(not(feature = "source-graph"))]
    {
        Vec::new()
    }
}

/// Extract one file through the registry, falling back to a file-level node.
///
/// This is the entry point every caller should use: it applies the size cap,
/// picks the extractor, and guarantees a file keeps file-level metadata no
/// matter which degraded path it takes.
pub fn extract_file(
    extractors: &[Box<dyn SourceGraphExtractor + Send + Sync>],
    path: &Path,
    bytes: &[u8],
) -> FileExtraction {
    if bytes.len() > MAX_EXTRACTED_FILE_BYTES {
        return FileExtraction::file_level(
            path,
            bytes,
            lexical::language_for_path(path),
            lexical::LEXICAL_PARSER_VERSION.to_string(),
            FileCoverage::Oversized {
                bytes: bytes.len(),
                limit: MAX_EXTRACTED_FILE_BYTES,
            },
        );
    }

    for extractor in extractors {
        if !extractor.supports(path) {
            continue;
        }
        let parser_version = extractor.cache_identity().to_parser_version();
        return match extractor.extract(path, bytes) {
            Ok(extraction) => extraction,
            // An extractor that fails outright must not remove the file from
            // the graph — degrade to the same file-level shape as an
            // unsupported language, naming the failure.
            Err(error) => FileExtraction::file_level(
                path,
                bytes,
                NodeLanguage::from(extractor.language()),
                parser_version,
                FileCoverage::LexicalOnly {
                    detail: format!("extractor failed: {error}"),
                },
            ),
        };
    }

    lexical::extract(path, bytes)
}

#[cfg(test)]
mod tests;
