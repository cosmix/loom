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
    }
}

fn request(budget_tokens: usize) -> PackRequest {
    PackRequest {
        query: "query".into(),
        scope: vec![Channel::Knowledge],
        budget_tokens,
        structural_freshness: Freshness::default(),
        semantic_freshness: Freshness::default(),
    }
}

#[test]
fn rule_25_packer_looks_up_chunks_by_their_string_ids() {
    let chunks = vec![chunk("b", "body", 1), chunk("a", "body", 1)];
    let packed = pack(
        &request(1),
        &[candidate("a", Channel::Knowledge, 1.0, 1)],
        &chunks,
    );
    assert_eq!(packed.items[0].pointer.path, PathBuf::from("a.md"));
}

#[test]
fn rule_26_missing_candidates_are_skipped_and_omitted() {
    let packed = pack(
        &request(10),
        &[candidate("missing", Channel::Knowledge, 1.0, 4)],
        &[],
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
    );
    assert!(packed.items.is_empty());
    assert_eq!(packed.omitted.omitted, 1);
}

#[test]
fn rule_29_included_candidates_become_fully_mapped_context_items() {
    let mut source = chunk("a", &"é".repeat(121), 4);
    source.heading.clear();
    source.anchor = "where".into();
    source.file = PathBuf::from("doc/a.md");
    source.state = LifecycleState::Draft;
    let ranked = RankedCandidate {
        id: ChunkId::from("a"),
        channel: Channel::Source,
        score: 2.5,
        reasons: vec![SelectionReason::ExactPath],
        token_count: 4,
    };
    let packed = pack(&request(4), &[ranked], &[source]);
    let item = &packed.items[0];
    assert_eq!(item.id.as_str(), "a");
    assert_eq!(item.kind, ItemKind::KnowledgeChunk);
    assert_eq!(item.pointer.path, PathBuf::from("doc/a.md"));
    assert_eq!(item.pointer.anchor, "where");
    assert_eq!(item.pointer.line_start, None);
    assert_eq!(item.pointer.line_end, None);
    assert_eq!(item.summary.chars().count(), 120);
    assert_eq!(item.source, Channel::Source);
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
    );
    assert!((packed.omitted.weakest_included_score - 0.4).abs() < 1e-4);
    let empty = pack(&request(0), &[], &[]);
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
    );
    assert!(packed.items.is_empty());
    assert_eq!(packed.omitted.omitted, 2);
    assert_eq!(packed.omitted.coverage.candidates, 2);
    assert_eq!(packed.omitted.coverage.included, 0);
    assert_eq!(packed.omitted.coverage.candidate_tokens, 1);
    assert_eq!(packed.omitted.coverage.included_tokens, 0);
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
        let packed = pack(&request(budget), &ranked, &chunks);
        assert!(packed.estimated_tokens <= packed.budget_tokens);
        assert!(packed.within_budget());
        assert_eq!(
            packed.omitted.coverage.included + packed.omitted.omitted,
            packed.omitted.coverage.candidates
        );
    }
}
