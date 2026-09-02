//! The rescue floor under corpus-derived stopwording: the empty-survivor floor
//! (A.16) and the thin-survivor floor that followed it.
//!
//! Split out of `rank_stopwords.rs` when the thin-survivor cases pushed that
//! file past the line limit. The base filter's own rules — what the ubiquity
//! floor drops, what backticks keep, what `dropped_terms` reports — stay there;
//! everything about putting a dropped term BACK reads here, together with the
//! corpus shaped to make a rescue observable.

use super::lexical_index::{assert_identical, rescued_source_graph};
use super::rank_fixtures::chunk;
use crate::context::config::RetrievalConfig;
use crate::context::lexical_index::LexicalCache;
use crate::context::rank::{rank_channel, ChannelRanking, RankQuery};
use crate::context::rank_source::{rank_source_channel, rank_source_channel_cached};
use crate::context::schema::{Channel, KnowledgeChunk};
use tempfile::TempDir;

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

/// [`layered_corpus`] with one chunk rewritten to hold a corpus-rare word, so a
/// query can carry exactly one surviving term and nothing else. `chunk-095` is
/// above every layer's cutoff already, so replacing its body changes no other
/// term's document frequency.
fn layered_corpus_with_a_rare_chunk() -> Vec<KnowledgeChunk> {
    let mut corpus = layered_corpus();
    corpus[95].body = "beryllium".to_string();
    corpus
}

/// Rank `text` against `corpus` with the default tunables.
fn rank_knowledge(text: &str, corpus: &[KnowledgeChunk]) -> ChannelRanking {
    rank_channel(
        &RankQuery {
            text: text.to_string(),
            ..RankQuery::default()
        },
        corpus,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )
}

/// Rank `text` against [`layered_corpus`] with the default tunables.
fn rank_layered(text: &str) -> ChannelRanking {
    rank_knowledge(text, &layered_corpus())
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

/// A short prompt is a lookup, not a question, and its one surviving term is
/// the word it was looking things up by. Two content terms is under
/// `RESCUE_QUERY_MIN_TERMS`, so the thin-survivor floor does not apply and
/// `settings` stays dropped exactly as it did before that floor existed.
#[test]
fn a_short_query_with_one_surviving_term_is_left_alone() {
    let ranking = rank_knowledge("settings beryllium", &layered_corpus_with_a_rare_chunk());

    assert_eq!(
        ranking.candidates.len(),
        1,
        "only the chunk matching the surviving term: {:?}",
        ranking.candidates
    );
    assert_eq!(ranking.candidates[0].id.as_str(), "chunk-095");
    assert_eq!(ranking.dropped_terms, vec!["settings".to_string()]);
}

/// The measured gap the thin-survivor floor closes, in fixture form: a question
/// of seven content words that stopwording reduced to ONE, which the empty-set
/// floor left alone because that one word was technically a survivor. The real
/// case was "sandbox settings rules for claude code worktree sessions", where
/// only `rules` survived and the expected chunk — which never says "rules" —
/// was not even a candidate.
///
/// The cap and the ordering still hold on this path: four dropped terms are
/// under the ceiling and three come back, rarest first, `alpha` before
/// `settings` on the tie at 20 documents.
#[test]
fn a_long_query_left_with_one_term_rescues_the_rarest_three() {
    let ranking = rank_knowledge(
        "the worktree sessions settings alpha rules beryllium",
        &layered_corpus_with_a_rare_chunk(),
    );

    assert_eq!(
        ranking.candidates.len(),
        21,
        "the 20 chunks saying `settings` or `alpha` — which subsume the 15 \
         saying `rules` — plus the one saying `beryllium`. 1 would mean the \
         thin-survivor floor never fired"
    );
    assert_eq!(
        ranking.dropped_terms,
        vec![
            "the".to_string(),
            "worktree".to_string(),
            "sessions".to_string()
        ],
        "`sessions` is under the ceiling but is the fourth-rarest of four \
         rescuable terms, so the cap of three leaves it dropped"
    );
}

/// The floor is measured in DISTINCT surviving terms, because that is what the
/// hook's emit floor counts: a prompt that says one rare word twice has covered
/// one term, and BM25 scoring it twice does not make a second. The rescue fires
/// here, and the repeat is left intact in the scored set.
#[test]
fn a_repeated_survivor_does_not_count_as_two_and_the_rescue_still_fires() {
    let ranking = rank_knowledge(
        "the worktree sessions beryllium beryllium",
        &layered_corpus_with_a_rare_chunk(),
    );

    assert_eq!(
        ranking.candidates.len(),
        26,
        "the 25 chunks saying `sessions`, the only rescuable term, plus the \
         one saying `beryllium`"
    );
    assert_eq!(
        ranking.dropped_terms,
        vec!["the".to_string(), "worktree".to_string()]
    );
}

/// `min_knowledge_terms` distinct survivors is enough for an item to clear the
/// hook's emit floor on lexical overlap alone, so the question is answerable as
/// it stands and nothing is put back — however many of its words the corpus
/// happens to have grown around.
#[test]
fn a_long_query_keeping_two_terms_is_left_alone() {
    let mut corpus = layered_corpus_with_a_rare_chunk();
    corpus[96].body = "cesium".to_string();

    let ranking = rank_knowledge("the worktree sessions beryllium cesium", &corpus);

    assert_eq!(
        ranking.candidates.len(),
        2,
        "only the two chunks matching a surviving term: {:?}",
        ranking.candidates
    );
    assert_eq!(
        ranking.dropped_terms,
        vec![
            "the".to_string(),
            "worktree".to_string(),
            "sessions".to_string()
        ],
        "two distinct survivors clear the emit floor, so the rescue must not \
         fire and every dropped term stays reported"
    );
}

/// A hundred chunks in which three ubiquitous words sit ABOVE the rescue
/// ceiling (25 of 100) — `the` at 90, `worktree` at 60, `sessions` at 30 — one
/// word sits between the ubiquity floor and that ceiling (`signal` at 15), and
/// one chunk holds a corpus-rare word. The shape that shows the ceiling still
/// binding while the thin-survivor floor is demonstrably running.
fn flooded_corpus() -> Vec<KnowledgeChunk> {
    (0..100)
        .map(|index| {
            let mut body = String::from("section body");
            for (documents, word) in [
                (90, "the"),
                (60, "worktree"),
                (30, "sessions"),
                (15, "signal"),
            ] {
                if index < documents {
                    body.push(' ');
                    body.push_str(word);
                }
            }
            if index == 95 {
                body.push_str(" beryllium");
            }
            chunk(&format!("chunk-{index:03}"), &body, 10)
        })
        .collect()
}

/// The ceiling binds the thin-survivor path exactly as it binds the empty one.
/// A term in 30% of the corpus is ubiquitous under any reading, and no amount
/// of thinning the survivors is allowed to argue otherwise — while `signal`,
/// dropped for ubiquity but under the ceiling, does come back. Asserting BOTH
/// halves is what makes this test fail if the thin-survivor branch is never
/// reached: a rescue that refuses everything and a rescue that never ran are
/// otherwise indistinguishable from the outside.
#[test]
fn a_thin_survivor_rescue_still_refuses_terms_above_the_ceiling() {
    let ranking = rank_knowledge("the worktree sessions signal beryllium", &flooded_corpus());

    assert_eq!(
        ranking.candidates.len(),
        16,
        "the 15 chunks saying `signal`, the only rescuable term, plus the one \
         saying `beryllium`. 1 would mean the thin-survivor floor never fired; \
         31 or more that a term above the ceiling came back with it"
    );
    assert_eq!(
        ranking.dropped_terms,
        vec![
            "the".to_string(),
            "worktree".to_string(),
            "sessions".to_string()
        ]
    );
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
        8,
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
