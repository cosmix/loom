//! Source-channel packing tests for [`crate::context::pack::pack`].

use super::source_fixtures::{full_node, graph_with_node, source_candidate};
use crate::context::pack::*;
use crate::context::rank::*;
use crate::context::schema::*;
use std::path::PathBuf;

fn request(budget_tokens: usize) -> PackRequest {
    PackRequest {
        query: "query".into(),
        scope: vec![Channel::Source],
        budget_tokens,
        structural_freshness: Freshness::default(),
        semantic_freshness: Freshness::default(),
    }
}

/// A `Channel::Source` candidate backed by a real `SourceNode` yields
/// `ItemKind::SourceNode` with a pointer carrying real line numbers. Every
/// other field is pinned exhaustively by `test_source_item_carries_every_field`.
#[test]
fn rule_29_included_candidates_become_fully_mapped_context_items() {
    let mut node = full_node(
        "src/a.rs#function:widget",
        "src/a.rs",
        &["widget"],
        "fn widget()",
    );
    node.span.line_start = 10;
    node.span.line_end = 12;
    let ranked = source_candidate("src/a.rs#function:widget", 2.5, 4);

    let packed = pack(&request(4), &[ranked], &[], Some(&graph_with_node(node)));
    let item = &packed.items[0];

    assert_eq!(item.kind, ItemKind::SourceNode);
    assert_eq!(item.pointer.path, PathBuf::from("src/a.rs"));
    assert_eq!(item.pointer.line_start, Some(10));
    assert_eq!(item.pointer.line_end, Some(12));
}

/// Pinned gate: every one of `ContextItem`'s twelve fields is carried from the
/// backing `SourceNode`/candidate — `kind == SourceNode` alone would not catch
/// a field silently left at `Default`.
#[test]
fn test_source_item_carries_every_field() {
    let mut node = full_node(
        "src/pack.rs#function:run",
        "src/pack.rs",
        &["widget", "run"],
        "pub fn run(query: &str) -> Widget",
    );
    node.span.line_start = 42;
    node.span.line_end = 57;
    let ranked = RankedCandidate {
        id: ChunkId::from(node.id.as_str()),
        channel: Channel::Source,
        score: 3.75,
        reasons: vec![SelectionReason::ExactSymbol, SelectionReason::Lexical],
        token_count: 22,
    };

    let packed = pack(
        &request(22),
        std::slice::from_ref(&ranked),
        &[],
        Some(&graph_with_node(node.clone())),
    );
    let item = packed
        .items
        .first()
        .expect("the source item must fit the budget");

    assert_identity_and_pointer(item, &node);
    assert_provenance_and_lifecycle(item, &node, &ranked);
}

/// First half of the pinned gate: id, kind, pointer and summary.
fn assert_identity_and_pointer(item: &ContextItem, node: &SourceNode) {
    assert_eq!(item.id, ChunkId::from(node.id.as_str()));
    assert_eq!(item.kind, ItemKind::SourceNode);
    assert_eq!(item.pointer.path, node.path);
    assert_eq!(item.pointer.line_start, Some(node.span.line_start));
    assert_eq!(item.pointer.line_end, Some(node.span.line_end));
    assert_eq!(
        item.pointer.anchor, "",
        "a source pointer has no heading anchor"
    );
    assert_eq!(
        item.summary,
        format!(
            "{} {} - {}:{}-{}",
            node.kind.as_str(),
            node.scope.join("::"),
            node.path.display(),
            node.span.line_start,
            node.span.line_end
        )
    );
}

/// A signature far longer than [`EXCERPT_MAX_TOKENS`] (400 estimated tokens,
/// ~1600 bytes at 4 bytes/token) must come back truncated. The short
/// signature above cannot tell `bounded_excerpt` apart from
/// `crate::utils::truncate_for_display` — both would return it unchanged —
/// so this is the only test that actually pins `build_source_item`'s
/// documented truncation contract.
#[test]
fn rule_29_a_long_signature_excerpt_is_truncated_and_marked() {
    let long_signature: String = (0..80)
        .map(|i| format!("pub fn overflow_variant_{i:03}(a: u32, b: u32) -> Widget;\n"))
        .collect();
    assert!(
        long_signature.len() > 1600,
        "fixture signature must exceed the excerpt bound, got {} bytes",
        long_signature.len()
    );

    let node = full_node(
        "src/a.rs#function:overflow",
        "src/a.rs",
        &["overflow"],
        &long_signature,
    );
    let ranked = source_candidate("src/a.rs#function:overflow", 1.0, 500);

    let packed = pack(&request(500), &[ranked], &[], Some(&graph_with_node(node)));
    let item = packed
        .items
        .first()
        .expect("the source item must fit the budget");
    let excerpt = item
        .excerpt
        .as_deref()
        .expect("a source item always carries an excerpt");

    let marker_line = format!("\n{EXCERPT_TRUNCATION_MARKER}");
    assert!(
        excerpt.ends_with(&marker_line),
        "expected the excerpt to end with the truncation marker on its own line, got: {excerpt}"
    );
    let prefix = excerpt.strip_suffix(&marker_line).unwrap();
    assert!(
        long_signature.starts_with(prefix),
        "the excerpt's head must be a verbatim prefix of the signature"
    );
    assert!(
        prefix.len() < long_signature.len(),
        "the excerpt must actually be shorter than the full signature"
    );
}

/// Second half of the pinned gate: provenance, scoring, lifecycle and excerpt.
fn assert_provenance_and_lifecycle(
    item: &ContextItem,
    node: &SourceNode,
    ranked: &RankedCandidate,
) {
    assert_eq!(item.source, Channel::Source);
    assert_eq!(item.token_count, ranked.token_count);
    assert!(
        (item.score - ranked.score).abs() < 1e-6,
        "got {}",
        item.score
    );
    assert_eq!(item.reasons, ranked.reasons);
    assert_eq!(item.confidence, Confidence::High);
    assert_eq!(item.state, LifecycleState::Active);
    assert_eq!(item.content_hash, node.body_hash);
    assert_eq!(item.excerpt.as_deref(), Some(node.signature.as_str()));
}
