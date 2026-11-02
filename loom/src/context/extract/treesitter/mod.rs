//! The shared tree-sitter walk every language extractor runs.
//!
//! A language module supplies a [`QueryHarness`] — a grammar, a query, and a
//! capture-name-to-kind mapping — and this module does the rest. Centralizing
//! the walk is what makes the evidence rules *structural* rather than a
//! convention every language implementation has to remember:
//!
//! - a definition nested inside another definition gets the outer one's scope,
//!   plus any qualifier written on the definition itself;
//! - `Contains` edges are [`EdgeProvenance::Structural`], the only class that
//!   carries `1.0`, because containment is syntactic and local;
//! - a `self`/`this` call bound to exactly one member of the enclosing type is
//!   an [`EdgeProvenance::Receiver`] edge;
//! - a plain or qualified spelling with exactly one definition in lexical
//!   scope in this file is an [`EdgeProvenance::LocalName`] edge;
//! - anything else — every import, a call on any other receiver, a name an
//!   import binds, an ambiguous or unknown spelling — is an
//!   [`EdgeProvenance::Syntax`] edge to [`UNRESOLVED_TARGET`], keeping as
//!   `candidates` the ids an ambiguous spelling could mean. Cross-file
//!   resolution decides those.
//!
//! Every `Calls`, `Imports` and `References` edge carries the span of each
//! site it stands for; equal edges merge their sites. Declarations that share
//! an id get signature-derived suffixes (`ids`).
//!
//! ## Capture protocol
//!
//! | Capture                 | Meaning                                            |
//! | ----------------------- | -------------------------------------------------- |
//! | `@definition.<kind>`    | the whole definition, `<kind>` per [`QueryHarness::kind_for_capture`] |
//! | `@name`                 | the identifier naming the definition in that match |
//! | `@definition.qualifier` | in a definition match: a qualifier written on the definition (`W` in C++ `void W::run()`), inserted into its scope before its name |
//! | `@import.path`          | the module path of an import statement             |
//! | `@import.statement`     | the whole import statement; with the path it goes to [`QueryHarness::import_bindings`] |
//! | `@call.name`            | the callee at a call site, bare or qualified       |
//! | `@call.receiver`        | in a `@call.name` match: the receiver of a member call |
//! | `@reference.name`       | a non-call use that becomes a `References` edge    |
//!
//! A `@definition.*` match with no `@name` is counted toward
//! [`FileCoverage::Partial`] rather than emitted as an anonymous node.
//!
//! [`EdgeProvenance::Structural`]: crate::context::source_graph::EdgeProvenance::Structural
//! [`EdgeProvenance::Receiver`]: crate::context::source_graph::EdgeProvenance::Receiver
//! [`EdgeProvenance::LocalName`]: crate::context::source_graph::EdgeProvenance::LocalName
//! [`EdgeProvenance::Syntax`]: crate::context::source_graph::EdgeProvenance::Syntax
//! [`UNRESOLVED_TARGET`]: crate::context::source_graph::UNRESOLVED_TARGET

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tree_sitter::{Language, Parser, Query, Tree};

use super::dialect::dialect_by_id;
use super::{ExtractorIdentity, FileExtraction};
use crate::context::source_graph::{
    FileCoverage, ImportBinding, NodeLanguage, SourceNodeKind, Span,
};

mod binding;
mod build;
mod collect;
mod ids;
mod receiver_context;

use binding::BindingRules;
use build::build;
use collect::{collect, first_error};

/// Everything a language contributes to the shared walk.
pub trait QueryHarness {
    /// The pinned grammar.
    fn language(&self) -> Language;

    /// The embedded query source. Must use the capture protocol above.
    ///
    /// Compiled queries are cached per process, keyed by
    /// `(node_language, query_source)`, so that pair must identify the
    /// grammar: two harnesses sharing it must share [`QueryHarness::language`].
    fn query_source(&self) -> &'static str;

    /// Identity of this extractor build.
    fn identity(&self) -> ExtractorIdentity;

    /// Language tag stamped onto every node.
    fn node_language(&self) -> NodeLanguage;

    /// Map a `definition.<suffix>` capture suffix to a node kind. Returning
    /// `None` makes the match a partial-coverage miss instead of a node.
    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind>;

    /// The names one import statement binds. `statement` is the
    /// `@import.statement` text, `path` the module spec that
    /// [`QueryHarness::import_spec`] returned for it (Ruby `./x`, a PHP group
    /// member's full path), and `site` the span of the `@import.path` node.
    /// The default binds the whole module under its last path segment.
    fn import_bindings(&self, _statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        vec![default_import_binding(path, site)]
    }

    /// The scope segment a definition's `@name` text stands for. Identity by
    /// default; a grammar whose name text is not one segment rewrites it (PHP
    /// `namespace A\B;` is the single segment `A.B`).
    fn definition_name(&self, name: &str) -> String {
        name.to_string()
    }

    /// The module spec of one import: `path` is the normalized `@import.path`
    /// and `statement` the `@import.statement` text. Identity by default; a
    /// grammar whose path is not the whole spec completes it (Ruby
    /// `require_relative 'x'` is `./x`). Bindings and the `Imports` edge both
    /// carry the result.
    fn import_spec(&self, _statement: &str, path: &str) -> String {
        path.to_string()
    }

    /// Receiver spellings that mean "the enclosing type". Defaults to the
    /// `self_receivers` column of the dialect whose id is
    /// [`QueryHarness::node_language`].
    fn self_receivers(&self) -> &'static [&'static str] {
        dialect_by_id(self.node_language().as_str())
            .map(|dialect| dialect.self_receivers)
            .unwrap_or_default()
    }

    /// Whether a self receiver outside every type names the file's top
    /// level, so `self.run()` there binds like a bare `run()`. False by
    /// default: such a call stays unbound, like a call on any other receiver.
    fn top_level_self(&self) -> bool {
        false
    }
}

/// The binding of an import match that has no `@import.statement`, and of
/// [`QueryHarness::import_bindings`] by default: the whole module, not a glob.
fn default_import_binding(path: &str, site: Span) -> ImportBinding {
    ImportBinding {
        path: path.to_string(),
        name: None,
        alias: None,
        glob: false,
        exported_as: None,
        site,
    }
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

    let query = compiled_query(harness, &node_language)?;

    let rules = binding_rules(harness, &node_language);
    let walk = collect(harness, &query, root, bytes);
    Ok(build(
        path,
        bytes,
        node_language,
        parser_version,
        walk,
        &rules,
    ))
}

/// Compiled queries, one per language and query source for the whole process.
/// Compiling is far costlier than running a query, and every file of a
/// dialect runs the same one.
type QueryCache = Mutex<HashMap<(String, &'static str), Arc<Query>>>;

/// `harness`'s query, compiled on first use. A query that fails to compile is
/// not cached, so every call reports the error.
fn compiled_query(harness: &dyn QueryHarness, node_language: &NodeLanguage) -> Result<Arc<Query>> {
    static CACHE: OnceLock<QueryCache> = OnceLock::new();
    let key = (node_language.as_str().to_string(), harness.query_source());
    let mut cache = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(query) = cache.get(&key) {
        return Ok(Arc::clone(query));
    }
    let query = Query::new(&harness.language(), key.1)
        .map_err(|error| anyhow!("invalid tree-sitter query for {node_language}: {error}"))?;
    Ok(Arc::clone(cache.entry(key).or_insert(Arc::new(query))))
}

/// The same-file binding rules `harness` runs under: its own receiver
/// spellings and top-level `self`, and whether its dialect lets a bare call
/// reach a member.
fn binding_rules(harness: &dyn QueryHarness, node_language: &NodeLanguage) -> BindingRules {
    BindingRules {
        self_receivers: harness.self_receivers(),
        bare_calls_reach_members: dialect_by_id(node_language.as_str())
            .is_some_and(|dialect| dialect.bare_calls_reach_members),
        top_level_self: harness.top_level_self(),
    }
}

/// Parse `bytes`, returning `None` when the grammar produced no tree at all.
fn parse(harness: &dyn QueryHarness, bytes: &[u8]) -> Result<Option<Tree>> {
    let mut parser = Parser::new();
    parser
        .set_language(&harness.language())
        .map_err(|error| anyhow!("failed to load grammar: {error}"))?;
    Ok(parser.parse(bytes, None))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_identity;
#[cfg(test)]
mod tests_receivers;
