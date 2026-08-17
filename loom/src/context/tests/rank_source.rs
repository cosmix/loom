use super::source_fixtures::{full_node, graph, node};
use crate::context::graph_store::ResolvedGraph;
use crate::context::rank::RankQuery;
use crate::context::rank_source::rank_source;
use crate::context::schema::{Confidence, FileCoverage, SelectionReason, SourceNodeKind};

/// A node whose terminal scope segment appears in the query text earns the
/// exact-symbol boost and must outrank a node that only shares a lexical term.
#[test]
fn test_exact_symbol_outranks_lexical_only() {
    let query = RankQuery {
        text: "explain the widget cache behavior".to_string(),
        ..RankQuery::default()
    };
    let mut exact = full_node(
        "src/a.rs#function:widget",
        "src/a.rs",
        &["Widget"],
        "fn widget()",
    );
    exact.span.line_start = 10;
    let mut lexical = full_node(
        "src/b.rs#function:helper",
        "src/b.rs",
        &["helper"],
        "fn helper(cache: bool)",
    );
    lexical.span.line_start = 20;

    let candidates = rank_source(
        &query,
        &graph(vec![("src/a.rs", vec![exact]), ("src/b.rs", vec![lexical])]),
    );

    assert_eq!(
        candidates.len(),
        2,
        "both nodes should match: {candidates:?}"
    );
    assert_eq!(
        candidates[0].id.as_str(),
        "src/a.rs#function:widget",
        "exact-symbol candidate must sort first: {candidates:?}"
    );
    assert!(
        candidates[0].score > candidates[1].score,
        "exact-symbol candidate must outscore lexical-only: {candidates:?}"
    );
}

/// A node whose file was not fully extracted can never claim a high-confidence
/// exact match, even when its symbol and path both literally appear in the
/// query text — [`FileCoverage::Full`] is the only coverage that earns the
/// exact rungs.
#[test]
fn test_non_full_coverage_never_reaches_high_confidence() {
    let query = RankQuery {
        text: "inspect src/context/widget.rs Widget carefully".to_string(),
        ..RankQuery::default()
    };
    let degraded = node(
        "src/context/widget.rs#function:widget",
        "src/context/widget.rs",
        &["Widget"],
        "fn widget()",
        SourceNodeKind::Function,
        FileCoverage::Partial {
            detail: "12 query matches had no named capture".to_string(),
        },
    );

    let candidates = rank_source(
        &query,
        &graph(vec![("src/context/widget.rs", vec![degraded])]),
    );

    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id.as_str() == "src/context/widget.rs#function:widget")
        .expect("the degraded node still earns a lexical match and must be present");
    assert_ne!(
        Confidence::from_reasons(&candidate.reasons),
        Confidence::High,
        "partial coverage must never earn high confidence: {candidate:?}"
    );
    assert!(
        !candidate.reasons.contains(&SelectionReason::ExactSymbol),
        "the exact-symbol rung must be withheld: {candidate:?}"
    );
    assert!(
        !candidate.reasons.contains(&SelectionReason::ExactPath),
        "the exact-path rung must be withheld: {candidate:?}"
    );
}

/// A required id withholds its explicit-id rung when its file's coverage isn't
/// full — but a required id that loses its rung must still be ranked and
/// still appear. Pinned acceptance gate for `rank_source::withhold_partial_coverage`.
#[test]
fn test_explicit_id_on_non_full_coverage_is_not_high() {
    let query = RankQuery {
        text: "trace payment retries".to_string(),
        required_ids: vec!["src/billing.rs#function:retry".to_string()],
        ..RankQuery::default()
    };
    let degraded = node(
        "src/billing.rs#function:retry",
        "src/billing.rs",
        &["retry"],
        "fn retry(payment: Payment)",
        SourceNodeKind::Function,
        FileCoverage::LexicalOnly {
            detail: "no extractor".to_string(),
        },
    );

    let candidates = rank_source(&query, &graph(vec![("src/billing.rs", vec![degraded])]));

    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id.as_str() == "src/billing.rs#function:retry")
        .expect("a required id that loses its rung must still be ranked");
    assert_ne!(
        Confidence::from_reasons(&candidate.reasons),
        Confidence::High,
        "an explicit id on non-full coverage must not reach high confidence: {candidate:?}"
    );
    assert!(
        !candidate.reasons.contains(&SelectionReason::ExplicitId),
        "the explicit-id rung must be withheld on non-full coverage: {candidate:?}"
    );
}

/// Three nodes with identical scope/signature text — so their BM25 documents,
/// and therefore their scores, are genuinely identical — but distinct
/// `(path, line_start)`, so the tie-break actually has ties to decide.
fn tied_nodes_graph() -> ResolvedGraph {
    let mut node_b = full_node(
        "src/b.rs#function:helper1",
        "src/b.rs",
        &["helper"],
        "fn helper(value: i32)",
    );
    node_b.span.line_start = 50;
    let mut node_a5 = full_node(
        "src/a.rs#function:helper5",
        "src/a.rs",
        &["helper"],
        "fn helper(value: i32)",
    );
    node_a5.span.line_start = 5;
    let mut node_a1 = full_node(
        "src/a.rs#function:helper1",
        "src/a.rs",
        &["helper"],
        "fn helper(value: i32)",
    );
    node_a1.span.line_start = 1;
    graph(vec![
        ("src/a.rs", vec![node_a5, node_a1]),
        ("src/b.rs", vec![node_b]),
    ])
}

/// Two runs over identical bytes must produce identical output, and the
/// `(path, line_start)` tie-break must actually decide ties rather than being
/// masked by distinct scores.
#[test]
fn test_ordering_is_deterministic_across_runs() {
    let query = RankQuery {
        text: "call helper with value".to_string(),
        ..RankQuery::default()
    };
    let fixture = tied_nodes_graph();

    let first = rank_source(&query, &fixture);
    let second = rank_source(&query, &fixture);

    assert_eq!(
        first.len(),
        3,
        "all three tied nodes should match: {first:?}"
    );
    assert!(
        (first[0].score - first[1].score).abs() < 1e-6
            && (first[1].score - first[2].score).abs() < 1e-6,
        "the three candidates must be genuinely tied on score: {first:?}"
    );
    let expected_order = [
        "src/a.rs#function:helper1",
        "src/a.rs#function:helper5",
        "src/b.rs#function:helper1",
    ];
    let ids: Vec<&str> = first
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect();
    assert_eq!(
        ids, expected_order,
        "the tie-break must order by (path, line_start): {first:?}"
    );

    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.id, b.id, "repeated runs must agree on order");
        assert!(
            (a.score - b.score).abs() < 1e-6,
            "repeated runs must agree on score: {a:?} vs {b:?}"
        );
    }
}

/// A query whose terms appear in no node's scope or signature must return
/// normally rather than panicking: `score_bm25` indexes
/// `document_frequencies` by query term, a `BTreeMap` index that panics on a
/// missing key, so `prepare_lexical` must insert a zero-frequency entry for
/// every query term.
#[test]
fn test_query_term_matching_no_node_does_not_panic() {
    let query = RankQuery {
        text: "zzzznonexistentzzzz".to_string(),
        ..RankQuery::default()
    };
    let plain = full_node(
        "src/a.rs#function:plain",
        "src/a.rs",
        &["plain"],
        "fn plain()",
    );

    let candidates = rank_source(&query, &graph(vec![("src/a.rs", vec![plain])]));

    assert!(
        candidates.is_empty(),
        "no reason should fire for a term matching nothing: {candidates:?}"
    );
}
