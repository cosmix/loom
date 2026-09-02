//! The shared tree-sitter walk every language extractor runs.
//!
//! A language module supplies a [`QueryHarness`] — a grammar, a query, and a
//! capture-name-to-kind mapping — and this module does the rest. Centralizing
//! the walk is what makes the provenance rules *structural* rather than a
//! convention four separate implementations each have to remember:
//!
//! - a definition nested inside another definition gets the outer one's scope;
//! - `Contains` edges are always parser-resolved, because containment is
//!   syntactic and local;
//! - a call resolved to a definition **in the same file** is a parser edge;
//! - anything else — every import, and every call to a name this file does not
//!   define — is [`EdgeProvenance::Inferred`] and capped at
//!   [`MAX_INFERRED_CONFIDENCE`].
//!
//! ## Capture protocol
//!
//! | Capture               | Meaning                                              |
//! | --------------------- | ---------------------------------------------------- |
//! | `@definition.<kind>`  | the whole definition, `<kind>` per [`QueryHarness::kind_for_capture`] |
//! | `@name`               | the identifier naming the definition in that match   |
//! | `@import.path`        | the module path of an import statement               |
//! | `@call.name`          | the callee at a call site, bare or qualified         |
//!
//! A `@definition.*` match with no `@name` is counted toward
//! [`FileCoverage::Partial`] rather than emitted as an anonymous node.

use anyhow::{anyhow, Result};
use std::path::Path;
use tree_sitter::{Language, Parser, Query, Tree};

use super::{ExtractorIdentity, FileExtraction};
use crate::context::source_graph::{FileCoverage, NodeLanguage, SourceNodeKind, Span};

mod build;
mod collect;

use build::build;
use collect::{collect, first_error};

/// Confidence for an import edge: the statement is unambiguous, but the file it
/// names is not resolved here, so it stays inferred.
const IMPORT_CONFIDENCE: f32 = 0.5;
/// Confidence for a call whose callee this file does not define.
const UNRESOLVED_CALL_CONFIDENCE: f32 = 0.3;

/// Everything a language contributes to the shared walk.
pub trait QueryHarness {
    /// The pinned grammar.
    fn language(&self) -> Language;

    /// The embedded query source. Must use the capture protocol above.
    fn query_source(&self) -> &'static str;

    /// Identity of this extractor build.
    fn identity(&self) -> ExtractorIdentity;

    /// Language tag stamped onto every node.
    fn node_language(&self) -> NodeLanguage;

    /// Map a `definition.<suffix>` capture suffix to a node kind. Returning
    /// `None` makes the match a partial-coverage miss instead of a node.
    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind>;
}

/// Run `harness` over `bytes` and produce the file's extraction.
pub fn run_query(harness: &dyn QueryHarness, path: &Path, bytes: &[u8]) -> Result<FileExtraction> {
    let identity = harness.identity();
    let parser_version = identity.to_parser_version();
    let node_language = harness.node_language();

    let tree = match parse(harness, bytes)? {
        Some(tree) => tree,
        None => {
            return Ok(FileExtraction::file_level(
                path,
                bytes,
                node_language,
                parser_version,
                FileCoverage::ParseError {
                    span: Span::default(),
                    detail: "the grammar returned no parse tree".to_string(),
                },
            ));
        }
    };

    let root = tree.root_node();
    if root.has_error() {
        // A syntax error yields NO symbol nodes: half a tree is worse than an
        // honest gap, because a consumer cannot tell which half is missing.
        let (span, detail) = first_error(root, bytes);
        return Ok(FileExtraction::file_level(
            path,
            bytes,
            node_language,
            parser_version,
            FileCoverage::ParseError { span, detail },
        ));
    }

    let query = Query::new(&harness.language(), harness.query_source())
        .map_err(|error| anyhow!("invalid tree-sitter query for {node_language}: {error}"))?;

    let walk = collect(harness, &query, root, bytes);
    Ok(build(path, bytes, node_language, parser_version, walk))
}

/// Parse `bytes`, returning `None` when the grammar produced no tree at all.
fn parse(harness: &dyn QueryHarness, bytes: &[u8]) -> Result<Option<Tree>> {
    let mut parser = Parser::new();
    parser
        .set_language(&harness.language())
        .map_err(|error| anyhow!("failed to load grammar: {error}"))?;
    Ok(parser.parse(bytes, None))
}
