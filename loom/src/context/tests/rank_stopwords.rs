//! Corpus-derived query stopwording and the candidacy floor (A.2), the rescue
//! floor that keeps it from emptying a pack outright (A.16), plus the BM25
//! missing-key guard (A.18).
//!
//! Before this, `df > 0` for any shared term made a chunk a candidate — "the"
//! included — so essentially the whole corpus was a candidate on every prompt,
//! the "Omitted: N weaker matches" footer reported corpus size rather than
//! anything omitted, and the document-frequency scan was paid over every
//! document twice per prompt.
//!
//! The rescue's cached-versus-scan agreement is asserted here too, beside the
//! rest of the stopwording rules rather than among `lexical_index.rs`'s index
//! tests, so that every consequence of one partition reads in one place.

use super::lexical_index::{assert_identical, rescued_source_graph};
use super::rank_fixtures::{chunk, rank_text};
use crate::context::config::RetrievalConfig;
use crate::context::lexical_index::LexicalCache;
use crate::context::rank::{rank_channel, score_bm25, tokenize, ChannelRanking, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use crate::context::schema::{Channel, KnowledgeChunk, SelectionReason};
use std::collections::BTreeMap;
use tempfile::TempDir;

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

/// One hundred chunks layered so that every term of a realistic question sits
/// ABOVE the ubiquity floor (10 of 100) while some sit under the rescue ceiling
/// (25 of 100) — the shape A.15's prose indexing gave the real corpus, where
/// `settings`, `rules` and `sessions` went from 57, 48 and 65 of 658 curated
/// documents to 105, 93 and 102 of 904 once prose joined them.
///
/// | term       | documents | floor of 10 | ceiling of 25            |
/// | ---------- | --------- | ----------- | ------------------------ |
/// | `the`      | 90        | dropped     | far above: never rescued |
/// | `worktree` | 26        | dropped     | one above: not rescued   |
/// | `sessions` | 25        | dropped     | exactly at it            |
/// | `settings` | 20        | dropped     | under it                 |
/// | `alpha`    | 20        | dropped     | under it, tied           |
/// | `rules`    | 15        | dropped     | under it                 |
/// | `signal`   | 12        | dropped     | under it                 |
fn layered_corpus() -> Vec<KnowledgeChunk> {
    (0..100)
        .map(|index| {
            let mut body = String::from("section body");
            for (documents, word) in [
                (90, "the"),
                (26, "worktree"),
                (25, "sessions"),
                (20, "settings"),
                (20, "alpha"),
                (15, "rules"),
                (12, "signal"),
            ] {
                if index < documents {
                    body.push(' ');
                    body.push_str(word);
                }
            }
            chunk(&format!("chunk-{index:03}"), &body, 10)
        })
        .collect()
}

/// Rank `text` against [`layered_corpus`] with the default tunables.
fn rank_layered(text: &str) -> ChannelRanking {
    rank_channel(
        &RankQuery {
            text: text.to_string(),
            ..RankQuery::default()
        },
        &layered_corpus(),
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )
}

/// The measured defect A.16 exists for: an ordinary, well-formed question whose
/// every term the corpus has grown around, which A.2 answered with an EMPTY
/// pack. The rescue puts back the three rarest terms still under the ceiling —
/// `signal` (12), `rules` (15), `settings` (20) — and leaves `sessions` (25,
/// under the ceiling but not among the three rarest) and `worktree` (26)
/// dropped, so the cap is not a blanket undo of stopwording.
#[test]
fn a_query_of_only_ubiquitous_terms_rescues_the_three_rarest() {
    let ranking = rank_layered("worktree sessions settings rules signal");

    assert_eq!(
        ranking.candidates.len(),
        20,
        "the candidates are the chunks holding a rescued term: the 20 saying \
         `settings`, which subsume the 15 saying `rules` and the 12 saying \
         `signal`. 25 would mean `sessions` was rescued over a rarer term, 0 \
         that nothing was"
    );
    assert_eq!(
        ranking.dropped_terms,
        vec!["worktree".to_string(), "sessions".to_string()],
        "a rescued term was never dropped and must not be reported as such; \
         the rest keep first-seen order"
    );
}

/// The ceiling is inclusive. `sessions` at exactly 25 of 100 comes back and
/// `worktree` one document above it does not — the boundary a `<` instead of a
/// `<=` would silently move.
#[test]
fn a_term_exactly_at_the_rescue_ceiling_is_rescued_and_one_above_is_not() {
    let ranking = rank_layered("worktree sessions");

    assert_eq!(
        ranking.candidates.len(),
        25,
        "every chunk saying `sessions`, and nothing that only says `worktree`"
    );
    assert_eq!(ranking.dropped_terms, vec!["worktree".to_string()]);
}

/// A.2's own behaviour has to survive its rescue: a question made entirely of
/// the corpus's commonest words describes nothing, and the correct pack for it
/// is still empty. Both terms here are above the ceiling, so there is nothing
/// rare enough to deserve resurrection.
#[test]
fn a_query_whose_terms_all_exceed_the_ceiling_still_yields_no_candidates() {
    let ranking = rank_layered("the worktree");

    assert!(
        ranking.candidates.is_empty(),
        "the rescue must not readmit terms the ceiling rejected, got {} candidates",
        ranking.candidates.len()
    );
    assert_eq!(
        ranking.dropped_terms,
        vec!["the".to_string(), "worktree".to_string()]
    );
}

/// The rescue is a floor under the EMPTY case, not a second chance for every
/// ubiquitous term: one surviving term keeps the rest dropped, exactly as
/// before A.16.
#[test]
fn one_surviving_term_suppresses_the_rescue_entirely() {
    let mut corpus = layered_corpus();
    corpus[95].body = "beryllium".to_string();

    let ranking = rank_channel(
        &RankQuery {
            text: "settings beryllium".to_string(),
            ..RankQuery::default()
        },
        &corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    assert_eq!(
        ranking.candidates.len(),
        1,
        "only the chunk matching the surviving term: {:?}",
        ranking.candidates
    );
    assert_eq!(ranking.candidates[0].id.as_str(), "chunk-095");
    assert_eq!(ranking.dropped_terms, vec!["settings".to_string()]);
}

/// Which terms a rescue puts back is part of the answer, so it is stable over
/// identical bytes. Four terms sit under the ceiling and only three fit, and
/// the two at 20 documents tie — a tie broken by the term itself, never by
/// iteration order, so `alpha` is rescued and `settings` is not.
#[test]
fn a_tie_in_document_frequency_is_broken_deterministically() {
    let first = rank_layered("settings alpha rules signal");
    let second = rank_layered("settings alpha rules signal");

    assert_eq!(
        first.dropped_terms,
        vec!["settings".to_string()],
        "three of four rescuable terms come back, rarest first, ties by term"
    );
    assert_eq!(first.dropped_terms, second.dropped_terms);
    assert_eq!(first.candidates, second.candidates);
}

/// The rescue runs inside the one partition both corpus representations call,
/// so a warm index and a cold scan must rescue the same terms and rank the same
/// nodes. A rescue implemented on one path only would make a prompt's answer
/// depend on whether a cache file happened to be warm — the divergence A.13's
/// property test exists to prevent, which that test cannot reach here because
/// its generator never produces a query whose every term is dropped.
#[test]
fn a_rescued_query_ranks_identically_warm_and_cold() {
    let source_graph = rescued_source_graph();
    let query = RankQuery {
        text: "manifest tokens".to_string(),
        ..RankQuery::default()
    };
    let config = RetrievalConfig::default();
    let temp = TempDir::new().unwrap();
    let cache = LexicalCache::source(temp.path(), &source_graph);

    let scanned = rank_source_channel(&query, &source_graph, &config);
    let miss = rank_source_channel_cached(&query, &source_graph, &config, Some(&cache));
    let hit = rank_source_channel_cached(&query, &source_graph, &config, Some(&cache));

    assert_eq!(
        scanned.candidates.len(),
        15,
        "the rescue must fire, or this compares three empty rankings"
    );
    assert!(
        scanned.dropped_terms.is_empty(),
        "both terms were rescued, so neither is reported dropped: {:?}",
        scanned.dropped_terms
    );
    assert_identical(&scanned, &miss, "rescued query (miss)");
    assert_identical(&scanned, &hit, "rescued query (hit)");
}
