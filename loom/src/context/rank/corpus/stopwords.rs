//! Which query terms are worth scoring: corpus-derived stopwording (A.2) and
//! the rescue floor that keeps it from answering an ordinary question with
//! nothing at all (A.16).
//!
//! Corpus-derived rather than a fixed English list: a list would catch "the"
//! and "is" and stop there, while the words that actually flood this retrieval
//! are the project's own — "loom", "stage", "signal", "context" appear in most
//! documents of a loom knowledge tree and discriminate nothing, and no English
//! list contains them. Deriving the set from document frequency absorbs both
//! classes at once, adapts to whatever corpus it is pointed at, and needs
//! nobody to maintain it.
//!
//! Both of [`super`]'s corpus representations — the fresh scan and the warm
//! index — reach [`partition_terms`] with the same document frequencies, the
//! same corpus size and the same raw query, so a cache hit and a cache miss
//! cannot disagree about which terms survived. That agreement is by
//! construction, not by coincidence: there is one partition, called from both.

use crate::context::config::RetrievalConfig;
use crate::context::lexical::{backtick_spans, occurs_backticked};
use std::collections::{BTreeMap, BTreeSet};

/// How many terms the rescue floor may resurrect at most.
///
/// Three: enough that the rarest content words of an ordinary question come
/// back together, few enough that the rescue can never quietly become a blanket
/// undo of stopwording on a long query. Independent of `min_query_token_len`,
/// which answers an unrelated question (is this token long enough to mean
/// anything at all), and deliberately not a tunable — no property of a corpus
/// makes four right.
///
/// It does have to stay at or above `min_knowledge_terms` (2), though: the
/// prompt hook only emits a pack when some knowledge item matched that many
/// DISTINCT surviving terms (`commands/hook/user_prompt_compose.rs:84`), so a
/// rescue of one term could restore candidates and still leave the hook silent.
const RESCUE_LIMIT: usize = 3;

/// Split the tokenized query into the terms worth scoring and the terms that
/// are not, returning `(surviving, dropped)`.
///
/// Repeats survive in `surviving` because BM25 sums a repeated term twice and
/// that weighting predates this filter; `dropped` is deduplicated in first-seen
/// order because it is shown to a human, and because determinism over identical
/// bytes is a hard requirement of the whole pipeline.
pub(super) fn partition_terms(
    query_terms: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    corpus_size: usize,
    raw_query: &str,
    config: &RetrievalConfig,
) -> (Vec<String>, Vec<String>) {
    let spans = backtick_spans(raw_query);
    let lower_query = raw_query.to_ascii_lowercase();
    let floor = ubiquity_floor(corpus_size, config);

    let mut surviving = Vec::new();
    let mut dropped = Vec::new();
    for term in query_terms {
        let backticked = occurs_backticked(&lower_query, &spans, term);
        let frequency = document_frequencies.get(term).copied().unwrap_or(0);
        let keep =
            backticked || (term.len() >= config.min_query_token_len && frequency as f32 <= floor);
        if keep {
            surviving.push(term.clone());
        } else if !dropped.contains(term) {
            dropped.push(term.clone());
        }
    }

    if surviving.is_empty() {
        let rescued = rescue_rarest(&dropped, document_frequencies, corpus_size, config);
        // Rebuilt from the query rather than collected out of `rescued` so a
        // term the prompt said twice is still scored twice, and so the order is
        // the query's: `dropped` is deduplicated for display, `surviving`
        // deliberately is not.
        surviving = query_terms
            .iter()
            .filter(|term| rescued.contains(*term))
            .cloned()
            .collect();
        // A rescued term was not dropped, and `ContextPack::dropped_terms` is
        // observability an agent reads back — it must describe what this pass
        // actually scored, not what it nearly did.
        dropped.retain(|term| !rescued.contains(term));
    }
    (surviving, dropped)
}

/// The rarest dropped terms to put back when stopwording dropped them ALL —
/// empty when nothing dropped is rare enough to deserve it.
///
/// A.2 allowed the surviving set to be empty, reasoning that a query of pure
/// stopwords describes nothing and should retrieve nothing. That reasoning
/// holds for "the the the" and broke the moment A.15 indexed the project's own
/// prose into the same corpus: the prose is written in the vocabulary the
/// questions are asked in, so it lifts exactly the query's terms past the
/// floor. Measured on this repository, "worktree claude code sandbox settings
/// rules sessions" ranked its expected chunk at 34 over the 658 curated
/// documents (floor 65.8: `settings` 57, `rules` 48, `sessions` 65 survived) and
/// returned an EMPTY pack over the 904 documents that include prose (floor 90.4:
/// the same three terms reached 105, 93 and 102). An ordinary, well-formed
/// question about the codebase retrieved nothing, and every document added
/// makes that more likely, not less.
///
/// The ceiling is what keeps this from being an undo. A term above
/// `stop_rescue_max_ratio` of the corpus is ubiquitous under any reading and
/// stays dropped however empty the query gets: `the` at 90% of the corpus is
/// never resurrected, `settings` at 11.6% is, but only when nothing else
/// survived. On a corpus small enough that the ceiling falls below
/// [`ubiquity_floor`] the rescue is simply vacuous — nothing dropped for
/// ubiquity can clear it — which is the right outcome, because on a small corpus
/// the floor's `df_ident_max` arm has already kept the query's terms.
///
/// The alternative — computing document frequencies over the curated documents
/// only, leaving prose out of the statistics — was considered and rejected.
/// `ExactGate::is_rare` (`context/lexical/evidence.rs:185`) reads THIS SAME df
/// map from the opposite end, to decide whether a candidate's name is
/// corpus-rare enough to claim an exact-match rung on its own. Two different
/// notions of "rare", one counting prose and one not, would let a single term be
/// simultaneously too common to score and rare enough to claim an exact symbol
/// match — a contradiction with no principled resolution at either call site.
/// One df map, plus a guarantee that the surviving set is non-empty whenever
/// anything is rescuable, keeps that notion shared.
///
/// Terms dropped for length rather than ubiquity are rescuable on the same
/// terms. A two-letter token is a poor query, but a poor query that retrieves
/// its rarest term beats one that retrieves nothing, and a short ubiquitous
/// token is stopped by the ceiling like any other.
fn rescue_rarest(
    dropped: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    corpus_size: usize,
    config: &RetrievalConfig,
) -> BTreeSet<String> {
    let ceiling = corpus_size as f32 * config.stop_rescue_max_ratio;
    let mut rarest: Vec<(usize, &String)> = dropped
        .iter()
        .map(|term| (document_frequencies.get(term).copied().unwrap_or(0), term))
        .filter(|(frequency, _)| *frequency as f32 <= ceiling)
        .collect();
    // Document frequency ascending, then the term itself, because df alone does
    // not order ties and two runs over identical bytes must rescue an identical
    // set — the same determinism `dropped_terms` is held to.
    rarest.sort_unstable();
    rarest
        .into_iter()
        .take(RESCUE_LIMIT)
        .map(|(_, term)| term.clone())
        .collect()
}

/// Highest document frequency a term may reach and still be scored.
///
/// `corpus_size * stop_df_ratio` is the rule the ratio names, and on a real
/// corpus (thousands of documents) it is the only one that binds. The
/// `df_ident_max` floor exists for the other end: on a five-document corpus the
/// ratio resolves to 0.5, so a term in ONE document is "ubiquitous" and a small
/// knowledge tree would lose lexical retrieval entirely. A term that occurs in
/// at most `df_ident_max` documents is corpus-RARE by the very definition the
/// exact-rung gate uses — calling the same term ubiquitous here would have the
/// two ends of one df map contradict each other.
fn ubiquity_floor(corpus_size: usize, config: &RetrievalConfig) -> f32 {
    (corpus_size as f32 * config.stop_df_ratio).max(config.df_ident_max as f32)
}
