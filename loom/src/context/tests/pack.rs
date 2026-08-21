use crate::context::pack::*;
use crate::context::rank::*;
use crate::context::schema::*;
use std::path::PathBuf;

fn chunk(id: &str, body: &str, tokens: usize) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.to_string(),
        file: PathBuf::from(format!("{id}.md")),
        anchor: format!("{id}-anchor"),
        heading: format!("{id} heading"),
        body: body.to_string(),
        content_hash: String::new(),
        estimated_tokens: tokens,
        aliases: Vec::new(),
        category: None,
        source_paths: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
        state: LifecycleState::Active,
    }
}

fn candidate(id: &str, channel: Channel, score: f32, token_count: usize) -> RankedCandidate {
    RankedCandidate {
        id: ChunkId::from(id),
        channel,
        score,
        reasons: vec![SelectionReason::Lexical],
        token_count,
        matched_term_count: 1,
        confidence_ceiling: None,
    }
}

fn request(budget_tokens: usize) -> PackRequest {
    PackRequest {
        query: "query".into(),
        scope: vec![Channel::Knowledge],
        budget_tokens,
        structural_freshness: Freshness::default(),
        semantic_freshness: Freshness::default(),
        dropped_terms: Vec::new(),
        degraded: None,
    }
}

#[test]
fn rule_25_packer_looks_up_chunks_by_their_string_ids() {
    let chunks = vec![chunk("b", "body", 1), chunk("a", "body", 1)];
    let packed = pack(
        &request(1),
        &[candidate("a", Channel::Knowledge, 1.0, 1)],
        &chunks,
        None,
    );
    assert_eq!(packed.items[0].pointer.path, PathBuf::from("a.md"));
}

#[test]
fn rule_26_missing_candidates_are_skipped_and_omitted() {
    let packed = pack(
        &request(10),
        &[candidate("missing", Channel::Knowledge, 1.0, 4)],
        &[],
        None,
    );
    assert!(packed.items.is_empty());
    assert_eq!(packed.omitted.omitted, 1);
}

#[test]
fn rule_27_nonfitting_chunks_are_skipped_while_later_ones_can_fit() {
    let chunks = vec![chunk("large", "body", 8), chunk("small", "body", 3)];
    let packed = pack(
        &request(5),
        &[
            candidate("large", Channel::Knowledge, 2.0, 8),
            candidate("small", Channel::Knowledge, 1.0, 3),
        ],
        &chunks,
        None,
    );
    assert_eq!(packed.items[0].id.as_str(), "small");
    assert_eq!(packed.omitted.omitted, 1);
}

#[test]
fn rule_28_chunk_larger_than_the_total_budget_is_omitted() {
    let packed = pack(
        &request(5),
        &[candidate("large", Channel::Knowledge, 1.0, 6)],
        &[chunk("large", "body", 6)],
        None,
    );
    assert!(packed.items.is_empty());
    assert_eq!(packed.omitted.omitted, 1);
}

/// A `Channel::Knowledge` candidate dispatches to a knowledge chunk item —
/// `line_start` stays `None` and the pointer carries the chunk's heading
/// anchor rather than a line range. The `Channel::Source` counterpart lives in
/// `pack_source.rs`.
#[test]
fn rule_29_knowledge_candidates_still_become_knowledge_chunk_items() {
    let mut source = chunk("a", &"é".repeat(121), 4);
    source.heading.clear();
    source.anchor = "where".into();
    source.file = PathBuf::from("doc/a.md");
    source.state = LifecycleState::Draft;
    let ranked = RankedCandidate {
        id: ChunkId::from("a"),
        channel: Channel::Knowledge,
        score: 2.5,
        reasons: vec![SelectionReason::ExactPath],
        token_count: 4,
        matched_term_count: 0,
        confidence_ceiling: None,
    };
    let packed = pack(&request(4), &[ranked], &[source], None);
    let item = &packed.items[0];
    assert_eq!(item.id.as_str(), "a");
    assert_eq!(item.kind, ItemKind::KnowledgeChunk);
    assert_eq!(item.pointer.path, PathBuf::from("doc/a.md"));
    assert_eq!(item.pointer.anchor, "where");
    assert_eq!(item.pointer.line_start, None);
    assert_eq!(item.pointer.line_end, None);
    assert_eq!(item.summary.chars().count(), 120);
    assert_eq!(item.source, Channel::Knowledge);
    assert_eq!(item.token_count, 4);
    assert!((item.score - 2.5).abs() < 1e-4, "got {}", item.score);
    assert_eq!(item.reasons, vec![SelectionReason::ExactPath]);
    assert_eq!(item.confidence, Confidence::High);
    assert_eq!(item.state, LifecycleState::Draft);
}

#[test]
fn rule_30_estimated_tokens_is_the_included_sum_and_within_budget() {
    let chunks = vec![chunk("a", "body", 3), chunk("b", "body", 4)];
    let packed = pack(
        &request(7),
        &[
            candidate("a", Channel::Knowledge, 2.0, 3),
            candidate("b", Channel::Knowledge, 1.0, 4),
        ],
        &chunks,
        None,
    );
    assert_eq!(packed.estimated_tokens, 7);
    assert!(packed.within_budget());
}

#[test]
fn rule_31_every_unincluded_ranked_candidate_is_counted() {
    let packed = pack(
        &request(1),
        &[
            candidate("missing", Channel::Knowledge, 3.0, 1),
            candidate("large", Channel::Knowledge, 2.0, 2),
            candidate("fits", Channel::Knowledge, 1.0, 1),
        ],
        &[chunk("large", "body", 2), chunk("fits", "body", 1)],
        None,
    );
    assert_eq!(packed.items.len(), 1);
    assert_eq!(packed.omitted.omitted, 2);
}

#[test]
fn rule_32_weakest_included_score_is_the_minimum_or_zero() {
    let packed = pack(
        &request(4),
        &[
            candidate("a", Channel::Knowledge, 0.9, 2),
            candidate("b", Channel::Knowledge, 0.4, 2),
        ],
        &[chunk("a", "body", 2), chunk("b", "body", 2)],
        None,
    );
    assert!((packed.omitted.weakest_included_score - 0.4).abs() < 1e-4);
    let empty = pack(&request(0), &[], &[], None);
    assert_eq!(empty.omitted.weakest_included_score, 0.0);
}

#[test]
fn rule_33_coverage_reports_all_and_included_candidate_tokens() {
    let packed = pack(
        &request(8),
        &[
            candidate("a", Channel::Knowledge, 3.0, 5),
            candidate("b", Channel::Knowledge, 2.0, 7),
            candidate("c", Channel::Knowledge, 1.0, 3),
        ],
        &[
            chunk("a", "body", 5),
            chunk("b", "body", 7),
            chunk("c", "body", 3),
        ],
        None,
    );
    assert_eq!(packed.omitted.coverage.candidates, 3);
    assert_eq!(packed.omitted.coverage.included, 2);
    assert_eq!(packed.omitted.coverage.candidate_tokens, 15);
    assert_eq!(packed.omitted.coverage.included_tokens, 8);
}

#[test]
fn rule_34_zero_budget_returns_an_empty_pack_with_coverage() {
    let packed = pack(
        &request(0),
        &[
            candidate("free", Channel::Knowledge, 2.0, 0),
            candidate("costly", Channel::Knowledge, 1.0, 1),
        ],
        &[chunk("free", "body", 0), chunk("costly", "body", 1)],
        None,
    );
    assert!(packed.items.is_empty());
    assert_eq!(packed.omitted.omitted, 2);
    assert_eq!(packed.omitted.coverage.candidates, 2);
    assert_eq!(packed.omitted.coverage.included, 0);
    assert_eq!(packed.omitted.coverage.candidate_tokens, 1);
    assert_eq!(packed.omitted.coverage.included_tokens, 0);
}

/// Pack a single chunk whose body is `body`, returning the item it produced.
fn item_for_body(body: &str) -> ContextItem {
    let mut source = chunk("a", body, 1);
    source.content_hash = "sha256:deadbeef".to_string();
    let packed = pack(
        &request(1),
        &[candidate("a", Channel::Knowledge, 1.0, 1)],
        &[source],
        None,
    );
    packed.items.into_iter().next().expect("chunk should fit")
}

/// The verbatim text a truncated excerpt quotes, with the marker line removed.
/// Panics unless the marker really does sit alone on the final line.
fn quoted_prefix(excerpt: &str) -> &str {
    excerpt
        .strip_suffix(EXCERPT_TRUNCATION_MARKER)
        .expect("a truncated excerpt must announce itself with the marker")
        .strip_suffix('\n')
        .expect("the truncation marker must sit on its own line")
}

#[test]
fn packed_items_carry_the_backing_chunks_content_hash() {
    assert_eq!(item_for_body("body").content_hash, "sha256:deadbeef");
}

#[test]
fn a_body_within_the_excerpt_bound_is_quoted_unchanged() {
    let body = "## Heading\n\nA short section body.\n";
    let item = item_for_body(body);
    assert_eq!(item.excerpt.as_deref(), Some(body));
}

#[test]
fn a_body_over_the_excerpt_bound_is_cut_at_a_line_and_marked() {
    let line = "a line of prose that says something\n";
    let body = line.repeat(200);
    assert!(estimate_tokens(&body) > EXCERPT_MAX_TOKENS);

    let excerpt = item_for_body(&body).excerpt.expect("excerpt");
    assert!(excerpt.len() < body.len());
    let quoted = quoted_prefix(&excerpt);
    assert!(body.starts_with(quoted), "the excerpt must quote verbatim");
    // Cut back to a line boundary, so no quoted line is half a source line.
    assert!(quoted.ends_with("prose that says something"));
}

#[test]
fn an_excerpt_cut_landing_inside_a_multi_byte_character_does_not_panic() {
    // The byte limit is 1600, which is not a multiple of 3, so the naive cut
    // lands inside a '→'. Slicing a `&str` there panics; the packer walks back.
    let body = "→".repeat(700);
    let excerpt = item_for_body(&body).excerpt.expect("excerpt");
    let quoted = quoted_prefix(&excerpt);
    assert!(body.starts_with(quoted));
    assert_eq!(quoted.chars().count(), 533, "cut to the last whole '→'");

    // The same trap with a 2-byte character pushed off alignment by a 1-byte
    // prefix, so the limit lands mid-character from the other parity.
    let body = format!("a{}", "é".repeat(900));
    let excerpt = item_for_body(&body).excerpt.expect("excerpt");
    let quoted = quoted_prefix(&excerpt);
    assert!(body.starts_with(quoted));
    assert_eq!(quoted.chars().count(), 800, "'a' plus 799 whole 'é'");
}

#[test]
fn property_pack_never_exceeds_budget() {
    fn next(seed: &mut u32) -> u32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *seed
    }

    for iteration in 0..200_u32 {
        let mut seed = iteration.wrapping_add(1);
        let count = (next(&mut seed) % 40) as usize;
        let budget = (next(&mut seed) % 1_001) as usize;
        let mut chunks = Vec::new();
        let mut ranked = Vec::new();
        for index in 0..count {
            let tokens = (next(&mut seed) % 501) as usize;
            let id = format!("chunk-{index}");
            chunks.push(chunk(&id, "body", tokens));
            ranked.push(candidate(
                &id,
                Channel::Knowledge,
                next(&mut seed) as f32,
                tokens,
            ));
        }
        let packed = pack(&request(budget), &ranked, &chunks, None);
        assert!(packed.estimated_tokens <= packed.budget_tokens);
        assert!(packed.within_budget());
        assert_eq!(
            packed.omitted.coverage.included + packed.omitted.omitted,
            packed.omitted.coverage.candidates
        );
    }
}
