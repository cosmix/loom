//! Corpus-derived query stopwording and the candidacy floor (A.2), plus the
//! BM25 missing-key guard (A.18).
//!
//! Before this, `df > 0` for any shared term made a chunk a candidate — "the"
//! included — so essentially the whole corpus was a candidate on every prompt,
//! the "Omitted: N weaker matches" footer reported corpus size rather than
//! anything omitted, and the document-frequency scan was paid over every
//! document twice per prompt.
//!
//! Putting a dropped term BACK is [`super::rank_stopwords_rescue`]'s subject,
//! including the rescue's cached-versus-scan agreement.

use super::rank_fixtures::{chunk, rank_text};
use crate::context::config::RetrievalConfig;
use crate::context::rank::{rank_channel, score_bm25, tokenize, RankQuery};
use crate::context::schema::{Channel, KnowledgeChunk, SelectionReason};
use std::collections::BTreeMap;

/// One hundred chunks, ninety of which say "the" — the shape a real corpus has
/// and a hand-built two-chunk fixture never does. Ten of them additionally say
/// "alpha", putting `df("alpha")` exactly on the `stop_df_ratio` boundary.
fn hundred_chunk_corpus() -> Vec<KnowledgeChunk> {
    (0..100)
        .map(|index| {
            let mut body = String::from("section body");
            if index < 90 {
                body.push_str(" the");
            }
            if index < 10 {
                body.push_str(" alpha");
            }
            chunk(&format!("chunk-{index:03}"), &body, 10)
        })
        .collect()
}

/// A query made entirely of the corpus's own ubiquitous vocabulary describes
/// nothing, and the correct pack for it is empty. Returning ninety chunks
/// ranked by how often they say "the" is worse than returning none.
#[test]
fn a_prompt_of_only_ubiquitous_words_yields_no_candidates() {
    let ranking = rank_channel(
        &RankQuery {
            text: "the the the".to_string(),
            ..RankQuery::default()
        },
        &hundred_chunk_corpus(),
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    assert!(
        ranking.candidates.is_empty(),
        "a corpus-ubiquitous term must not make every chunk a candidate, got {}",
        ranking.candidates.len()
    );
    assert_eq!(ranking.dropped_terms, vec!["the".to_string()]);
}

/// The threshold is strictly-greater, so a term sitting exactly on
/// `corpus_size * stop_df_ratio` survives. Ten of a hundred is exactly 0.10.
#[test]
fn a_term_exactly_on_the_ratio_is_kept() {
    let ranking = rank_channel(
        &RankQuery {
            text: "alpha".to_string(),
            ..RankQuery::default()
        },
        &hundred_chunk_corpus(),
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    assert_eq!(
        ranking.candidates.len(),
        10,
        "df == corpus_size * stop_df_ratio must be kept, not dropped"
    );
    assert!(ranking.dropped_terms.is_empty());
}

/// Backticks outrank every other rule, in both directions: they keep a term
/// that is too short AND too common to survive on its own.
#[test]
fn a_backticked_short_common_term_survives_both_filters() {
    let mut corpus = hundred_chunk_corpus();
    for entry in corpus.iter_mut().take(90) {
        entry.body.push_str(" at");
    }

    let ranking = rank_channel(
        &RankQuery {
            text: "what does `at` mean here".to_string(),
            ..RankQuery::default()
        },
        &corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    assert_eq!(
        ranking.candidates.len(),
        90,
        "a backticked term is scored whatever its length or df"
    );
    assert!(
        !ranking.dropped_terms.contains(&"at".to_string()),
        "a backticked term is never dropped: {:?}",
        ranking.dropped_terms
    );
}

/// A chunk whose only overlap with the prompt is a dropped term is not a
/// candidate at all — which is what makes `omitted` mean "relevant things that
/// did not fit" instead of "corpus size".
#[test]
fn a_chunk_matching_only_a_dropped_term_is_not_a_candidate() {
    let mut corpus = hundred_chunk_corpus();
    corpus[95].body = "the beryllium".to_string();

    let ranked = rank_text("the beryllium", &corpus);

    assert_eq!(
        ranked.len(),
        1,
        "only the chunk matching the surviving term may appear: {ranked:?}"
    );
    assert_eq!(ranked[0].id.as_str(), "chunk-095");
    assert_eq!(ranked[0].reasons, vec![SelectionReason::Lexical]);
}

/// `dropped_terms` is reported to a human through `--json` and `--explain`, so
/// it is deduplicated, ordered by first appearance in the query, and identical
/// across runs over identical bytes.
#[test]
fn dropped_terms_are_deduplicated_ordered_and_deterministic() {
    let corpus = hundred_chunk_corpus();
    let query = RankQuery {
        text: "the alpha the of the".to_string(),
        ..RankQuery::default()
    };

    let first = rank_channel(
        &query,
        &corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );
    let second = rank_channel(
        &query,
        &corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    assert_eq!(
        first.dropped_terms,
        vec!["the".to_string(), "of".to_string()],
        "first-seen order, one entry per distinct term"
    );
    assert_eq!(first.dropped_terms, second.dropped_terms);
    assert_eq!(first.candidates, second.candidates);
}

/// A query term with no entry in the document-frequency map scores nothing
/// rather than panicking. Stopwording means the scored term list and the map no
/// longer have to agree, and this runs inside a hook that must never disturb a
/// session.
#[test]
fn score_bm25_tolerates_a_term_absent_from_the_frequency_map() {
    let documents = vec![vec![("ghost".to_string(), 1.0)]];

    let (score, matched) = score_bm25(
        &tokenize("ghost"),
        &BTreeMap::new(),
        &documents,
        &[1],
        1.0,
        1.0,
        0,
    );

    assert_eq!(score, 0.0);
    assert_eq!(matched, 0);
}
