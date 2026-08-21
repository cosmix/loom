//! The document-agnostic corpus machinery both channels score through:
//! per-document weighted tokens, document frequencies, query-term stopwording,
//! and BM25.
//!
//! Split out of `rank.rs` so the knowledge ranker's chunk-specific ladder and
//! the statistics it scores against stay under the file line limit
//! independently. Nothing here knows whether a document came from a knowledge
//! chunk or a source-graph node, which is the point:
//! [`crate::context::rank_source`] builds its own `(term, weight)` documents and
//! then scores through exactly these statistics, so the two rankers cannot
//! drift apart in how they weigh a term.
//!
//! ## Two representations, one arithmetic
//!
//! A corpus reaches this module in one of two shapes: freshly tokenized by the
//! caller (the scan), or read back from the persistent inverted index
//! ([`crate::context::lexical_index`], A.13). They must score IDENTICALLY —
//! bit for bit, including `matched_term_count` — because which one a given
//! prompt gets depends on nothing but whether a cache file happened to be
//! warm. So the BM25 formula is written ONCE, in [`score_terms`], and the two
//! representations differ only in the closure that answers "what weight does
//! this document give this term?".

use super::{BM25_B, BM25_K1};
use crate::context::config::RetrievalConfig;
use crate::context::lexical::{backtick_spans, occurs_backticked};
use crate::context::lexical_index::{LexicalCache, LexicalIndex, QueryPostings};
use std::collections::{BTreeMap, BTreeSet};

/// BM25 lexical score for the document at `index`, summed across query terms.
///
/// The full-scan scorer, and the oracle the indexed path is checked against: it
/// depends on nothing but its arguments, so a test can pin it one input at a
/// time. Production reaches it through [`LexicalCorpus::score`], which is also
/// what picks between it and the indexed path.
///
/// Returns the score and how many DISTINCT query terms matched the document.
/// The count is de-duplicated because `query_terms` may legitimately repeat a
/// term (a prompt saying "cache" twice tokenizes to two entries), and a repeat
/// is not extra evidence — the hook's emit floor counts *how much of the query*
/// an item covers, not how often the caller typed a word. The BM25 sum is
/// deliberately left alone: a repeated term contributing twice is the existing
/// weighting, and changing it is a ranking decision, not a counting one.
///
/// A term missing from `document_frequencies` scores nothing rather than
/// panicking. This runs inside a hook contractually forbidden to disturb a
/// session, so an indexing panic here is an outage in the editor, not a test
/// failure — and since [`prepare_lexical`] now hands back only the SURVIVING
/// terms, "every term has an entry" is no longer an invariant anyone can lean
/// on anyway.
#[allow(clippy::too_many_arguments)]
pub(crate) fn score_bm25(
    query_terms: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    documents: &[Vec<(String, f32)>],
    lengths: &[usize],
    average_length: f32,
    corpus_size: f32,
    index: usize,
) -> (f32, usize) {
    score_terms(
        query_terms,
        document_frequencies,
        lengths[index],
        average_length,
        corpus_size,
        |term: &str| scanned_frequency(&documents[index], term),
    )
}

/// BM25 over one document, given a way to look its term weights up.
///
/// `weighted_frequency` returns `None` when the document does not contain the
/// term at all — which is NOT the same as a weight of zero: a term the document
/// lacks contributes neither score nor a `matched_term_count`.
fn score_terms(
    query_terms: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    length: usize,
    average_length: f32,
    corpus_size: f32,
    weighted_frequency: impl Fn(&str) -> Option<f32>,
) -> (f32, usize) {
    let mut lexical_score = 0.0;
    let mut matched_terms: BTreeSet<&str> = BTreeSet::new();
    for term in query_terms {
        let document_frequency = document_frequencies.get(term).copied().unwrap_or(0);
        if document_frequency == 0 {
            continue;
        }
        let Some(weighted) = weighted_frequency(term) else {
            continue;
        };
        let frequency = document_frequency as f32;
        let idf = (1.0 + (corpus_size - frequency + 0.5) / (frequency + 0.5)).ln();
        let normalization = BM25_K1 * (1.0 - BM25_B + BM25_B * length as f32 / average_length);
        lexical_score += idf * (weighted * (BM25_K1 + 1.0)) / (weighted + normalization);
        matched_terms.insert(term.as_str());
    }
    (lexical_score, matched_terms.len())
}

/// The summed weight `document` gives `term`, or `None` when it does not
/// contain it.
///
/// Written as an explicit fold rather than `filter().map().sum()` so the seed
/// and the accumulation order are visible: the persistent index reproduces this
/// exact sum, and it can only do that if it folds the same way, in the same
/// order, from the same `0.0`.
fn scanned_frequency(document: &[(String, f32)], term: &str) -> Option<f32> {
    let mut total = 0.0f32;
    let mut found = false;
    for (value, weight) in document {
        if value == term {
            total += weight;
            found = true;
        }
    }
    found.then_some(total)
}

/// How a prepared corpus answers term-weight questions.
enum LexicalDocuments {
    /// Per-document weighted tokens, derived this run. The correctness oracle.
    Scanned(Vec<Vec<(String, f32)>>),
    /// Postings read back from the persistent index, for the surviving terms.
    Indexed(QueryPostings),
}

/// Query-dependent corpus statistics that do not depend on what a document
/// *is*: the surviving query terms, per-document weighted tokens and lengths,
/// the corpus average length, and per-term document frequencies.
pub(crate) struct LexicalCorpus {
    /// Query terms that SURVIVED stopwording, in query order, repeats intact.
    /// These, and only these, are scored.
    query_terms: Vec<String>,
    /// The corpus itself, in whichever representation this pass obtained.
    /// Private, and reachable only through [`LexicalCorpus::score`]: a caller
    /// that reached past it would have to handle both representations, and the
    /// one that forgot to would silently score every document zero.
    documents: LexicalDocuments,
    /// Token count per document, in corpus order.
    lengths: Vec<usize>,
    /// Mean document length; `0.0` for an empty corpus.
    average_length: f32,
    /// How many documents contain each query term — EVERY tokenized term, not
    /// just the survivors. The dropped ones stay because the exact-rung gate
    /// (`lexical::ExactGate`) asks this map how rare a candidate's name is, and
    /// the names it most needs to reject are exactly the ubiquitous words
    /// stopwording just dropped. Serving `0` for "point" because "point" was
    /// dropped would readmit every prose word the gate exists to keep out.
    pub(crate) document_frequencies: BTreeMap<String, usize>,
    /// Query terms dropped before scoring, deduplicated in first-seen order.
    /// Reported on [`crate::context::schema::ContextPack::dropped_terms`].
    pub(crate) dropped_terms: Vec<String>,
}

impl LexicalCorpus {
    /// BM25 score and distinct-matched-term count for the document at `index`.
    ///
    /// The one way to score a prepared corpus. Both arms feed the same
    /// [`score_terms`], so the only difference between them is where a term's
    /// weight comes from.
    pub(crate) fn score(&self, corpus_size: f32, index: usize) -> (f32, usize) {
        match &self.documents {
            LexicalDocuments::Scanned(documents) => score_bm25(
                &self.query_terms,
                &self.document_frequencies,
                documents,
                &self.lengths,
                self.average_length,
                corpus_size,
                index,
            ),
            LexicalDocuments::Indexed(postings) => score_terms(
                &self.query_terms,
                &self.document_frequencies,
                self.lengths[index],
                self.average_length,
                corpus_size,
                |term: &str| postings.weighted_frequency(term, index),
            ),
        }
    }
}

/// Assemble the loop-invariant statistics for one ranking pass, dropping the
/// query terms that carry no information about this corpus.
///
/// Corpus-derived stopwording rather than a fixed English list: a list would
/// catch "the" and "is" and stop there, while the words that actually flood
/// this retrieval are the project's own — "loom", "stage", "signal", "context"
/// appear in most documents of a loom knowledge tree and discriminate nothing,
/// and no English list contains them. Deriving the set from document frequency
/// absorbs both classes at once, adapts to whatever corpus it is pointed at,
/// and needs nobody to maintain it.
pub(crate) fn prepare_lexical(
    query_terms: &[String],
    documents: Vec<Vec<(String, f32)>>,
    raw_query: &str,
    config: &RetrievalConfig,
) -> LexicalCorpus {
    let lengths: Vec<usize> = documents.iter().map(Vec::len).collect();

    // Document frequency per query term is loop-invariant: it scans every
    // document but does not depend on which document is currently being
    // scored. Computed once here instead of inside the per-document loop.
    let mut document_frequencies: BTreeMap<String, usize> = BTreeMap::new();
    for term in query_terms {
        document_frequencies.entry(term.clone()).or_insert_with(|| {
            documents
                .iter()
                .filter(|document| document.iter().any(|(value, _)| value == term))
                .count()
        });
    }

    let (surviving, dropped) = partition_terms(
        query_terms,
        &document_frequencies,
        lengths.len(),
        raw_query,
        config,
    );
    assemble(
        surviving,
        dropped,
        LexicalDocuments::Scanned(documents),
        lengths,
        document_frequencies,
    )
}

/// [`prepare_lexical`], but reading the persistent inverted index when it is
/// warm and building it when it is not.
///
/// `build_documents` is a closure, not a `Vec`, because skipping it IS the
/// optimization: tokenizing ~7,900 source nodes is the single largest cost in a
/// prompt-time retrieval, and a hit must not pay it. `doc_ids` are the corpus's
/// document identities in corpus order — cheap to collect, and what proves a
/// warm file describes THIS corpus and not a same-revision-different-documents
/// one.
///
/// A miss falls back to the full scan and then writes the index, best-effort:
/// see [`LexicalCache`], where every failure is a `debug!` line and never an
/// error, because a checkout that cannot write a cache must still retrieve.
pub(crate) fn prepare_lexical_cached<F>(
    query_terms: &[String],
    doc_ids: &[&str],
    build_documents: F,
    raw_query: &str,
    config: &RetrievalConfig,
    cache: Option<&LexicalCache>,
) -> LexicalCorpus
where
    F: FnOnce() -> Vec<Vec<(String, f32)>>,
{
    if let Some(index) = cache.and_then(|cache| cache.load(doc_ids)) {
        return from_index(query_terms, &index, raw_query, config);
    }
    let documents = build_documents();
    if let Some(cache) = cache {
        cache.save(&LexicalIndex::build(cache.revision(), doc_ids, &documents));
    }
    prepare_lexical(query_terms, documents, raw_query, config)
}

/// Prepare a corpus from a warm index instead of from documents.
///
/// `lengths` and the document frequencies are recomputed here rather than read
/// from the file — see `lexical_index`'s module docs on why a derived value is
/// not persisted — and the surviving/dropped partition runs on exactly the same
/// inputs it would have on the scan path, so the two agree by construction
/// rather than by coincidence.
fn from_index(
    query_terms: &[String],
    index: &LexicalIndex,
    raw_query: &str,
    config: &RetrievalConfig,
) -> LexicalCorpus {
    let lengths = index.lengths();
    let document_frequencies = index.document_frequencies(query_terms);
    let (surviving, dropped) = partition_terms(
        query_terms,
        &document_frequencies,
        lengths.len(),
        raw_query,
        config,
    );
    // Projected AFTER the partition so only the terms that will actually be
    // scored are decoded, and so the parsed index — megabytes of postings for
    // the whole vocabulary — can be dropped as this function returns.
    let postings = LexicalDocuments::Indexed(index.project(&surviving));
    assemble(surviving, dropped, postings, lengths, document_frequencies)
}

/// Finish a corpus once its terms are partitioned and its representation,
/// lengths and frequencies are known. The single place a [`LexicalCorpus`] is
/// constructed, so the two paths cannot disagree about a derived field.
fn assemble(
    query_terms: Vec<String>,
    dropped_terms: Vec<String>,
    documents: LexicalDocuments,
    lengths: Vec<usize>,
    document_frequencies: BTreeMap<String, usize>,
) -> LexicalCorpus {
    LexicalCorpus {
        query_terms,
        documents,
        average_length: mean_length(&lengths),
        lengths,
        document_frequencies,
        dropped_terms,
    }
}

/// Mean document length, `0.0` for an empty corpus.
///
/// One expression, called by both paths: BM25's length normalization divides by
/// this, so a scanned corpus and an indexed one computing it even one ULP apart
/// would score every document differently.
fn mean_length(lengths: &[usize]) -> f32 {
    if lengths.is_empty() {
        return 0.0;
    }
    lengths.iter().sum::<usize>() as f32 / lengths.len() as f32
}

/// Split the tokenized query into the terms worth scoring and the terms that
/// are not, returning `(surviving, dropped)`.
///
/// Repeats survive in `surviving` because BM25 sums a repeated term twice and
/// that weighting predates this filter; `dropped` is deduplicated in first-seen
/// order because it is shown to a human, and because determinism over identical
/// bytes is a hard requirement of the whole pipeline.
fn partition_terms(
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
    (surviving, dropped)
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
/// two halves of this module contradict each other.
fn ubiquity_floor(corpus_size: usize, config: &RetrievalConfig) -> f32 {
    (corpus_size as f32 * config.stop_df_ratio).max(config.df_ident_max as f32)
}
