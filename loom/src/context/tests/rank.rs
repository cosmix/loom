use super::rank_fixtures::chunk;
use crate::context::config::RetrievalConfig;
use crate::context::rank::*;
use crate::context::schema::*;

#[test]
fn rule_1_tokenizer_lowercases_input() {
    assert_eq!(tokenize("LOUD"), vec!["loud"]);
}

#[test]
fn rule_2_tokenizer_splits_non_ascii_alphanumeric_boundaries() {
    assert_eq!(tokenize("one,two/three!"), vec!["one", "two", "three"]);
}

#[test]
fn rule_3_tokenizer_emits_snake_case_parts() {
    assert_eq!(tokenize("some_value"), vec!["some_value", "some", "value"]);
}

#[test]
fn rule_4_tokenizer_emits_camel_case_parts_from_original_case() {
    assert_eq!(tokenize("camelCase"), vec!["camelcase", "camel", "case"]);
}

#[test]
fn rule_5_tokenizer_keeps_the_original_token() {
    assert_eq!(tokenize("keep_me"), vec!["keep_me", "keep", "me"]);
}

#[test]
fn rule_6_tokenizer_keeps_repeated_terms() {
    assert_eq!(tokenize("again again"), vec!["again", "again"]);
}

#[test]
fn rule_7_document_frequency_counts_each_chunk_once() {
    let chunks = vec![chunk("a", "alpha alpha", 1), chunk("b", "beta", 1)];
    let got = rank(
        &RankQuery {
            text: "alpha".into(),
            ..RankQuery::default()
        },
        &chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )[0]
    .score;
    assert!((got - 0.871_385).abs() < 1e-4, "got {got}");
}

#[test]
fn rule_8_idf_uses_the_specified_logarithm() {
    let chunks = vec![
        chunk("a", "alpha", 1),
        chunk("b", "beta", 1),
        chunk("c", "gamma", 1),
    ];
    let got = rank(
        &RankQuery {
            text: "alpha".into(),
            ..RankQuery::default()
        },
        &chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )[0]
    .score;
    assert!((got - 0.980_829).abs() < 1e-4, "got {got}");
}

#[test]
fn rule_9_weighted_fields_contribute_their_assigned_weights() {
    let mut first = chunk("a", "alpha", 1);
    first.heading = "alpha".into();
    first.aliases = vec!["alpha".into()];
    first.anchor = "alpha".into();
    first.symbols = vec!["alpha".into()];
    first.source_paths = vec!["alpha".into()];
    let mut second = chunk("b", "beta", 1);
    second.heading = "beta".into();
    second.aliases = vec!["beta".into()];
    second.anchor = "beta".into();
    second.symbols = vec!["beta".into()];
    second.source_paths = vec!["beta".into()];
    let got = rank(
        &RankQuery {
            text: "alpha".into(),
            ..RankQuery::default()
        },
        &[first, second],
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )[0]
    .score;
    // The BM25 component is 1.391_353. This fixture puts "alpha" in both
    // `source_paths` and `symbols`, and the query is literally "alpha", so the
    // exact-path rung (+100.0) fires too — additive and independent of the field
    // weighting under test. The exact-symbol rung does NOT: `alpha` is one
    // lowercase word, and since `lexical::is_plain_word` such a name can only be
    // admitted by back-ticks or spelling, never by being rare here.
    assert!((got - 101.391_36).abs() < 1e-4, "got {got}");
}

#[test]
fn rule_10_document_length_is_unweighted_across_fields() {
    let chunks = vec![chunk("a", "alpha filler", 1), chunk("b", "filler", 1)];
    let got = rank(
        &RankQuery {
            text: "alpha".into(),
            ..RankQuery::default()
        },
        &chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )[0]
    .score;
    assert!((got - 0.609_970).abs() < 1e-4, "got {got}");
}

#[test]
fn rule_11_bm25_sums_each_query_term_contribution() {
    let chunks = vec![chunk("a", "alpha beta", 1), chunk("b", "gamma delta", 1)];
    let got = rank(
        &RankQuery {
            text: "alpha beta".into(),
            ..RankQuery::default()
        },
        &chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )[0]
    .score;
    assert!((got - 1.386_294).abs() < 1e-4, "got {got}");
}

#[test]
fn rule_12_nonzero_bm25_adds_a_lexical_reason() {
    let chunks = vec![chunk("a", "alpha", 1), chunk("b", "beta", 1)];
    let ranked = rank(
        &RankQuery {
            text: "alpha".into(),
            ..RankQuery::default()
        },
        &chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );
    assert!(
        // IDF(q) = ln(1 + (2 - 1 + 0.5) / (1 + 0.5)) = ln(2) for a two-chunk
        // corpus where one chunk matches, and the BM25 factor here is 1.
        (ranked[0].score - std::f32::consts::LN_2).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(ranked[0].reasons, vec![SelectionReason::Lexical]);
}

// Rules 13-19, the `contains_whole_term` boundary regressions, and the
// link-target resolution tests live in `rank_ladder.rs` — the exact-match
// ladder is a distinct concern from the tokenizer and BM25 rules above.
