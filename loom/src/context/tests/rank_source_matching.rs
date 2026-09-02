//! Exact-rung matching and node-selection tests for [`crate::context::rank_source`].
//!
//! [`rank_source`]'s exact-match rungs search raw query text with
//! `contains_whole_term` rather than tokenized terms, so they must survive
//! shapes plain token equality would break: a `::`-qualified CamelCase
//! symbol, a whole path, and a bare path stem.

use super::source_fixtures::{full_node, graph, node};
use crate::context::config::RetrievalConfig;
use crate::context::rank::RankQuery;
use crate::context::rank_source::rank_source;
use crate::context::schema::{FileCoverage, SelectionReason, SourceNodeKind};

/// A `::`-qualified CamelCase symbol query fires `ExactSymbol` on a node
/// whose terminal scope segment is the trailing name — token equality would
/// split `Foo::Bar` into `foo`/`bar` and match neither.
#[test]
fn test_camel_case_qualified_symbol_matches_exact_symbol() {
    let mut symbol_node = full_node(
        "src/foo.rs#type:Foo::Bar",
        "src/foo.rs",
        &["Foo", "Bar"],
        "struct Bar",
    );
    symbol_node.span.line_start = 1;
    let query = RankQuery {
        text: "Foo::Bar changed recently".to_string(),
        ..RankQuery::default()
    };

    let result = rank_source(
        &query,
        &graph(vec![("src/foo.rs", vec![symbol_node])]),
        &RetrievalConfig::default(),
    );

    assert!(
        result[0].reasons.contains(&SelectionReason::ExactSymbol),
        "Foo::Bar must exact-symbol match a node whose terminal scope is Bar: {result:?}"
    );
}

/// A query containing a node's full path fires `ExactPath`.
#[test]
fn test_full_path_matches_exact_path() {
    let mut path_node = full_node(
        "src/context/pack.rs#function:pack",
        "src/context/pack.rs",
        &["pack"],
        "pub fn pack()",
    );
    path_node.span.line_start = 1;
    let query = RankQuery {
        text: "look at src/context/pack.rs for the packer".to_string(),
        ..RankQuery::default()
    };

    let result = rank_source(
        &query,
        &graph(vec![("src/context/pack.rs", vec![path_node])]),
        &RetrievalConfig::default(),
    );

    assert!(
        result[0].reasons.contains(&SelectionReason::ExactPath),
        "the full path must exact-path match: {result:?}"
    );
}

/// A bare file stem (no directory, no extension) also fires `ExactPath` — but
/// only through the gate, since a stem is just a word. `pack` is one lowercase
/// word, so back-ticks are what make this occurrence a code reference; written
/// bare it would be the English verb and earn nothing.
#[test]
fn test_bare_path_stem_matches_exact_path() {
    let mut stem_node = full_node(
        "src/context/pack.rs#function:pack",
        "src/context/pack.rs",
        &["pack"],
        "pub fn pack()",
    );
    stem_node.span.line_start = 1;
    let query = RankQuery {
        text: "`pack` budgets by score".to_string(),
        ..RankQuery::default()
    };

    let result = rank_source(
        &query,
        &graph(vec![("src/context/pack.rs", vec![stem_node])]),
        &RetrievalConfig::default(),
    );

    assert!(
        result[0].reasons.contains(&SelectionReason::ExactPath),
        "the bare stem must exact-path match too: {result:?}"
    );
}

/// A graph containing both a [`SourceNodeKind::File`] node and a real symbol
/// node in the SAME file returns ONLY the symbol: whole-file nodes carry no
/// signature and no scope, and would otherwise crowd out the symbols inside
/// their file.
///
/// A graph holding *only* the excluded node cannot fail if `rank_source`
/// returned nothing at all — this keeps a positive control (the symbol node,
/// sharing the file node's own path so it earns its OWN exact-path match) in
/// the same graph and asserts the result is exactly that one candidate, so
/// the test fails both if the exclusion breaks (the file node reappears,
/// outranking the symbol via its required-id boost) and if ranking breaks
/// entirely (the symbol disappears too).
#[test]
fn test_file_kind_nodes_are_excluded() {
    let query = RankQuery {
        text: "src/lib.rs".to_string(),
        required_ids: vec!["src/lib.rs".to_string()],
        ..RankQuery::default()
    };
    let file_node = node(
        "src/lib.rs",
        "src/lib.rs",
        &[],
        "",
        SourceNodeKind::File,
        FileCoverage::Full,
    );
    let mut symbol_node = full_node(
        "src/lib.rs#function:run",
        "src/lib.rs",
        &["run"],
        "pub fn run()",
    );
    symbol_node.span.line_start = 1;

    let candidates = rank_source(
        &query,
        &graph(vec![("src/lib.rs", vec![file_node, symbol_node])]),
        &RetrievalConfig::default(),
    );

    assert_eq!(
        candidates.len(),
        1,
        "the file-kind node must never become a candidate, even a required-id \
         match, and even alongside a real symbol in the same file: {candidates:?}"
    );
    assert_eq!(
        candidates[0].id.as_str(),
        "src/lib.rs#function:run",
        "the symbol node sharing the excluded node's file must still survive \
         selection: {candidates:?}"
    );
}

/// A node sharing no term with the query, not named in `required_ids`, and
/// matching neither path nor symbol is not a candidate at all — `score_node`
/// returns `None` rather than a zero-score entry.
///
/// The unrelated node sits in the SAME graph as a node that DOES match (by
/// bare path stem), so the test fails both if the exclusion breaks (the
/// unrelated node reappears) and if ranking breaks entirely (the positive
/// control disappears too) — a graph holding only the unrelated node could
/// not distinguish either regression from `rank_source` always returning
/// nothing.
#[test]
fn test_node_with_no_fired_reason_is_excluded() {
    let query = RankQuery {
        text: "completely unrelated query text about `pack`".to_string(),
        ..RankQuery::default()
    };
    let unrelated = full_node(
        "src/other.rs#function:doStuff",
        "src/other.rs",
        &["doStuff"],
        "fn do_stuff(x: i32)",
    );
    let mut matching = full_node(
        "src/context/pack.rs#function:pack",
        "src/context/pack.rs",
        &["pack"],
        "pub fn pack()",
    );
    matching.span.line_start = 1;

    let candidates = rank_source(
        &query,
        &graph(vec![
            ("src/other.rs", vec![unrelated]),
            ("src/context/pack.rs", vec![matching]),
        ]),
        &RetrievalConfig::default(),
    );

    assert_eq!(
        candidates.len(),
        1,
        "a node sharing nothing with the query must not appear, even \
         alongside one that does: {candidates:?}"
    );
    assert_eq!(
        candidates[0].id.as_str(),
        "src/context/pack.rs#function:pack",
        "the matching node must survive selection: {candidates:?}"
    );
}
