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
//! defined in another file is emitted as an unresolved syntax edge, never as
//! a bound edge. Cross-file resolution is `crate::context::resolve`'s
//! job, and it too only ever raises confidence with evidence.
//!
//! ## Degraded modes
//!
//! Nothing makes a file vanish from the graph:
//!
//! | Situation                              | Result                                      |
//! | -------------------------------------- | ------------------------------------------- |
//! | No dialect claims the extension        | file node, `FileCoverage::LexicalOnly`      |
//! | Dialect known, grammar not registered  | file node, `FileCoverage::LexicalOnly` (a named gap) |
//! | File over [`MAX_EXTRACTED_FILE_BYTES`] | file node, `FileCoverage::Oversized`        |
//! | Grammar reports a syntax error         | file node, `FileCoverage::ParseError`       |
//! | `source-graph` cargo feature disabled  | file node, `FileCoverage::LexicalOnly`      |

use anyhow::Result;
use std::path::Path;

use serde::Serialize;

use crate::context::source_graph::{
    body_hash, file_node_id, FileCoverage, ImportBinding, NodeLanguage, SourceEdge, SourceNode,
    SourceNodeKind, Span, MAX_EXTRACTED_FILE_BYTES,
};
use dialect::{dialect_for_path, DialectSpec};

pub mod dialect;
pub mod lexical;

#[cfg(feature = "source-graph-wave-c")]
pub mod c;
#[cfg(feature = "source-graph-wave-c")]
pub mod cpp;
#[cfg(feature = "source-graph-wave-b")]
pub mod csharp;
#[cfg(feature = "source-graph")]
mod ecmascript;
#[cfg(feature = "source-graph")]
pub mod go;
#[cfg(feature = "source-graph-wave-b")]
pub mod java;
#[cfg(feature = "source-graph")]
pub mod javascript;
#[cfg(feature = "source-graph-wave-b")]
pub mod php;
#[cfg(feature = "source-graph")]
pub mod python;
#[cfg(feature = "source-graph-wave-b")]
pub mod ruby;
#[cfg(feature = "source-graph")]
pub mod rust;
#[cfg(feature = "source-graph")]
pub mod tsx;
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
    /// Id of the dialect the extractor handles, so TypeScript and TSX never
    /// share a parser version.
    pub dialect: &'static str,
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
            "{}:{}+{}+v{}",
            self.dialect, self.grammar_version, short, self.extractor_version
        )
    }
}

/// Everything one file contributed to the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct FileExtraction {
    pub nodes: Vec<SourceNode>,
    pub edges: Vec<SourceEdge>,
    pub coverage: FileCoverage,
    /// Import bindings the file declares; empty for every file-level
    /// extraction.
    pub imports: Vec<ImportBinding>,
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
            imports: Vec::new(),
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
        symbol_key: String::new(),
    }
}

/// Span covering an entire buffer.
pub fn whole_file_span(bytes: &[u8]) -> Span {
    let lines = bytes.iter().filter(|byte| **byte == b'\n').count();
    Span {
        start_byte: 0,
        end_byte: bytes.len(),
        line_start: 1,
        line_end: whole_file_line_count(lines, bytes.last().copied()),
    }
}

/// Inclusive last line, with an empty file represented as line one.
pub(crate) fn whole_file_line_count(newlines: usize, last: Option<u8>) -> usize {
    (newlines + usize::from(last.is_some_and(|byte| byte != b'\n'))).max(1)
}

#[cfg(test)]
#[test]
fn whole_file_span_counts_unterminated_lines() {
    for (bytes, lines) in [(b"".as_slice(), 1), (b"a", 1), (b"a\nb", 2), (b"a\nb\n", 2)] {
        assert_eq!(whole_file_span(bytes).line_end, lines);
    }
}

/// What an extractor captures. A dimension an extractor does not claim is
/// reported as not covered instead of reading as "none found".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Definitions become nodes.
    pub declarations: bool,
    /// Import statements become `Imports` edges.
    pub imports: bool,
    /// Import statements also yield [`ImportBinding`]s.
    pub import_bindings: bool,
    /// Call sites become `Calls` edges.
    pub calls: bool,
    /// Member calls record their receiver text.
    pub receivers: bool,
    /// Non-call identifier mentions become `References` edges.
    pub references: bool,
}

/// One language's extraction strategy.
pub trait SourceGraphExtractor {
    /// The single dialect this extractor handles.
    fn dialect(&self) -> &'static DialectSpec;

    /// What this extractor captures.
    fn capabilities(&self) -> Capabilities;

    /// Identity of this extractor build, for cache invalidation.
    fn cache_identity(&self) -> ExtractorIdentity;

    /// Extract nodes and edges from `bytes`, which must be the exact contents
    /// of `path`.
    ///
    /// `path` is used for ids and must be relative to the project root.
    /// Implementations return `Err` only for a genuine internal failure —
    /// a syntax error in the input is data, reported as
    /// [`FileCoverage::ParseError`], not an error.
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}

/// One entry of the extractor registry the semantic refresh drives.
///
/// Boxed because the driver builds the whole registry once and hands slices of
/// it down its own call chain, which needs `Send + Sync` for no reason a single
/// extractor implementation cares about.
pub type BoxedExtractor = Box<dyn SourceGraphExtractor + Send + Sync>;

/// Every compiled-in extractor, in registration order.
///
/// Boxed rather than an enum so a host without the `source-graph` feature gets
/// an empty registry and the callers above it need no `cfg`.
pub fn registry() -> Vec<BoxedExtractor> {
    #[allow(unused_mut)]
    let mut extractors: Vec<BoxedExtractor> = Vec::new();
    #[cfg(feature = "source-graph")]
    {
        extractors.push(Box::new(rust::RustExtractor::new()));
        extractors.push(Box::new(typescript::TypeScriptExtractor::new()));
        extractors.push(Box::new(tsx::TsxExtractor::new()));
        extractors.push(Box::new(javascript::JavaScriptExtractor::new()));
        extractors.push(Box::new(python::PythonExtractor::new()));
        extractors.push(Box::new(go::GoExtractor::new()));
    }
    #[cfg(feature = "source-graph-wave-b")]
    {
        extractors.push(Box::new(java::JavaExtractor::new()));
        extractors.push(Box::new(csharp::CSharpExtractor::new()));
        extractors.push(Box::new(ruby::RubyExtractor::new()));
        extractors.push(Box::new(php::PhpExtractor::new()));
    }
    #[cfg(feature = "source-graph-wave-c")]
    {
        extractors.push(Box::new(c::CExtractor::new()));
        extractors.push(Box::new(cpp::CppExtractor::new()));
    }
    extractors
}

/// How a path maps onto the registry.
pub enum Lookup<'a> {
    /// A registered extractor handles the path's dialect.
    Extractor(&'a (dyn SourceGraphExtractor + Send + Sync)),
    /// The path's dialect is known but no extractor is available for it.
    Gap {
        dialect: &'static DialectSpec,
        /// Why: the grammar pack is not compiled in, or nothing is registered.
        detail: String,
    },
    /// No dialect claims the path's extension.
    Unknown,
}

/// Find the extractor for `path`: the dialect owning its extension, then the
/// registered extractor whose dialect matches. Precedence is the dialect table,
/// never registration order.
pub fn extractor_for<'a>(extractors: &'a [BoxedExtractor], path: &Path) -> Lookup<'a> {
    let Some(dialect) = dialect_for_path(path) else {
        return Lookup::Unknown;
    };
    match extractors
        .iter()
        .find(|extractor| extractor.dialect().id == dialect.id)
    {
        Some(extractor) => Lookup::Extractor(extractor.as_ref()),
        None => Lookup::Gap {
            dialect,
            detail: gap_detail(dialect),
        },
    }
}

/// The reason a known dialect has no extractor.
fn gap_detail(dialect: &DialectSpec) -> String {
    if dialect.pack.compiled() {
        format!("no extractor registered for dialect {}", dialect.id)
    } else {
        format!(
            "grammar pack {} not compiled in ({})",
            dialect.pack.feature(),
            dialect.id
        )
    }
}

/// Extract one file through the registry, falling back to a file-level node.
///
/// This is the entry point every caller should use: it applies the size cap,
/// picks the extractor, and guarantees a file keeps file-level metadata no
/// matter which degraded path it takes.
pub fn extract_file(extractors: &[BoxedExtractor], path: &Path, bytes: &[u8]) -> FileExtraction {
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

    match extractor_for(extractors, path) {
        Lookup::Extractor(extractor) => match extractor.extract(path, bytes) {
            Ok(extraction) => extraction,
            // An extractor that fails outright must not remove the file from
            // the graph — degrade to the same file-level shape as an
            // unsupported language, naming the failure.
            Err(error) => FileExtraction::file_level(
                path,
                bytes,
                extractor.dialect().language.clone(),
                extractor.cache_identity().to_parser_version(),
                FileCoverage::LexicalOnly {
                    detail: format!("extractor failed: {error}"),
                },
            ),
        },
        Lookup::Gap { dialect, detail } => FileExtraction::file_level(
            path,
            bytes,
            dialect.language.clone(),
            lexical::LEXICAL_PARSER_VERSION.to_string(),
            FileCoverage::LexicalOnly { detail },
        ),
        Lookup::Unknown => lexical::extract(path, bytes),
    }
}

#[cfg(test)]
mod tests;
