//! Tests for the curated-knowledge-over-prose scoring prior (A.15), which is
//! implemented as a DEMOTION of prose rather than a bonus to curated — see
//! [`super::prose_demotion`] for why the direction matters and why the caller
//! clamps the result at zero.
//!
//! Fixtures are hand-built here rather than pulled from
//! `crate::context::tests::rank_fixtures` deliberately: that module is owned
//! by a sibling worker landing alongside this one. The shape mirrors
//! `rank_fixtures::chunk` (`context/tests/rank_fixtures.rs`).

use super::*;
use crate::context::schema::LifecycleState;

/// Build a `KnowledgeChunk` with every field explicit but `body`/`id`, mirroring
/// `context/tests/rank_fixtures.rs::chunk` so the two stay easy to compare.
fn chunk(id: &str, body: &str) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.to_string(),
        file: PathBuf::from(format!("{id}.md")),
        anchor: String::new(),
        heading: String::new(),
        body: body.to_string(),
        content_hash: String::new(),
        estimated_tokens: 1,
        aliases: Vec::new(),
        category: None,
        source_paths: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
        state: LifecycleState::Active,
    }
}

fn rank_text(
    text: &str,
    chunks: &[KnowledgeChunk],
    config: &RetrievalConfig,
) -> Vec<RankedCandidate> {
    rank(
        &RankQuery {
            text: text.to_string(),
            ..RankQuery::default()
        },
        chunks,
        Channel::Knowledge,
        config,
    )
}

/// One candidate's score by id, panicking rather than silently skipping when
/// the id is absent — a `find(...).map(...)` that quietly yields `None` would
/// turn "the chunk stopped being a candidate" into a vacuously passing test.
fn score_of(candidates: &[RankedCandidate], id: &str) -> f32 {
    candidates
        .iter()
        .find(|candidate| candidate.id.as_str() == id)
        .unwrap_or_else(|| panic!("no candidate with id {id}: {candidates:?}"))
        .score
}

/// One curated chunk and one prose chunk with identical bodies, so the only
/// thing that can separate them is the prior.
fn curated_and_prose() -> (KnowledgeChunk, KnowledgeChunk) {
    let prose_id = format!("{PROSE_ID_PREFIX}doc/design.md#topic#0");
    (
        chunk("zeta.md#topic#0", "alpha beta"),
        chunk(&prose_id, "alpha beta"),
    )
}

#[test]
fn curated_outranks_prose_at_equal_evidence() {
    // The curated id ("zeta.md#topic#0") sorts AFTER the prose id
    // ("prose:doc/design.md#topic#0") in plain ascending-string order ('p' <
    // 'z'). That is the point: if the prior were removed, the existing
    // ascending-id tie-break in `by_score_then_id` would put the prose chunk
    // first by accident and this test would still fail loudly, proving the
    // pass is driven by the prior and not by coincidental id ordering.
    let (curated, prose) = curated_and_prose();

    let config = RetrievalConfig::default();
    let got = rank_text("alpha", &[curated.clone(), prose.clone()], &config);

    assert_eq!(got.len(), 2, "both chunks should match 'alpha'");
    assert_eq!(
        got[0].id.as_str(),
        curated.id,
        "curated chunk must outrank the prose chunk at equal evidence"
    );
}

#[test]
fn the_demotion_is_exactly_the_configured_prior_and_leaves_curated_alone() {
    // Calibrated against a zero-prior run rather than against a hard-coded
    // BM25 value: what this test owns is the SIZE and DIRECTION of the
    // adjustment, and spelling out an expected score would silently re-pin the
    // ranking arithmetic that `context/tests/rank.rs` and
    // `context/tests/rank_ladder.rs` exist to pin.
    let (curated, prose) = curated_and_prose();
    let chunks = [curated.clone(), prose.clone()];

    let undemoted = RetrievalConfig {
        knowledge_curated_prior: 0.0,
        ..RetrievalConfig::default()
    };
    // Read per chunk rather than assuming the two score alike: `field_tokens`
    // happens to ignore `id` and `file` (`context/lexical.rs:82`), so these
    // fixtures do score identically today, but nothing here needs that to hold.
    let baseline = rank_text("alpha", &chunks, &undemoted);
    let curated_baseline = score_of(&baseline, &curated.id);
    let prose_baseline = score_of(&baseline, &prose.id);
    assert!(
        prose_baseline > 0.0,
        "the fixture must score something for the subtraction to be visible"
    );

    // Half the undemoted prose score, so the clamp cannot bite and the whole
    // subtraction is observable; the saturating case is the test below.
    let config = RetrievalConfig {
        knowledge_curated_prior: prose_baseline / 2.0,
        ..RetrievalConfig::default()
    };
    let got = rank_text("alpha", &chunks, &config);

    assert!(
        (score_of(&got, &curated.id) - curated_baseline).abs() < 1e-6,
        "a curated score must be untouched by the prior — that is the whole \
         reason this is a demotion and not a bonus"
    );
    assert!(
        (score_of(&got, &prose.id) - (prose_baseline - config.knowledge_curated_prior)).abs()
            < 1e-6,
        "the prose score must drop by exactly the configured prior"
    );
}

#[test]
fn a_saturating_prior_clamps_the_prose_score_at_zero() {
    // A prior far larger than any BM25 score drives the subtraction negative.
    // It must clamp: `fuse::normalized_score` divides by the channel maximum
    // and guards a zero or non-finite divisor but NOT a negative one, so a
    // negative prose score on a prose-only query would invert the entire list
    // (`super::prose_demotion`'s doc comment carries the arithmetic).
    let (curated, prose) = curated_and_prose();

    let config = RetrievalConfig {
        knowledge_curated_prior: 500.0,
        ..RetrievalConfig::default()
    };
    let got = rank_text("alpha", &[curated.clone(), prose.clone()], &config);

    assert_eq!(
        got.len(),
        2,
        "the demotion orders candidates, it never un-admits one"
    );
    assert_eq!(
        got[0].id.as_str(),
        curated.id,
        "curated chunk must still rank first with a saturating prior"
    );

    let prose_score = score_of(&got, &prose.id);
    assert!(
        prose_score >= 0.0,
        "the clamp must never let a prose score go negative, got {prose_score}"
    );
    assert!(
        prose_score.abs() < 1e-6,
        "a prior larger than any score must clamp to zero, got {prose_score}"
    );
}

#[test]
fn the_prior_does_not_make_an_unmatched_chunk_a_candidate() {
    // Regression for the doc comment on `prose_demotion`: settling the prior
    // inside the exact-match ladder (rather than after `score_chunk` already
    // decided a chunk is a candidate) would turn every curated chunk into a
    // candidate on every query, even one that matches nothing in it.
    let (curated, prose) = curated_and_prose();

    let config = RetrievalConfig::default();
    let got = rank_text("nothing-in-either-chunk", &[curated, prose], &config);

    assert!(
        got.is_empty(),
        "a query matching neither chunk must yield no candidates, got {got:?}"
    );
}
