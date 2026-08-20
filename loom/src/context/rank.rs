//! Deterministic BM25 ranking for knowledge chunks, plus the document-agnostic
//! corpus machinery (`LexicalCorpus`, `prepare_lexical`, `score_bm25`)
//! that [`crate::context::rank_source()`] scores source-graph nodes through.

use crate::context::config::RetrievalConfig;
use crate::context::lexical::{contains_whole_term, field_tokens, link_target_matches};
use crate::context::schema::{Channel, ChunkId, KnowledgeChunk, SelectionReason};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Re-exported so callers (and this module's own tests) can tokenize text the
/// same way [`rank`] does without reaching into `crate::context::lexical`.
pub use crate::context::lexical::tokenize;

/// BM25 term-frequency saturation parameter.
pub const BM25_K1: f32 = 1.2;
/// BM25 document-length normalization parameter.
pub const BM25_B: f32 = 0.75;
/// Additive score for an explicitly required chunk id.
pub const BOOST_EXPLICIT_ID: f32 = 1000.0;
/// Additive score for an exact source-path substring.
pub const BOOST_EXACT_PATH: f32 = 100.0;
/// Additive score for an exact symbol substring.
pub const BOOST_EXACT_SYMBOL: f32 = 80.0;
/// Additive score for a direct link neighbour.
pub const BOOST_LINKED_FROM: f32 = 40.0;
/// Additive score for a stage dependency id.
pub const BOOST_STAGE_DEPENDENCY: f32 = 30.0;

/// What the caller is asking for.
#[derive(Debug, Clone, Default)]
pub struct RankQuery {
    /// Query text used for lexical and exact matching.
    pub text: String,
    /// Chunk ids the caller demands verbatim.
    pub required_ids: Vec<String>,
    /// Chunk ids referenced by stages this query depends on.
    pub stage_dependency_ids: Vec<String>,
    /// Project-relative paths owned by the stages this query depends on.
    ///
    /// Only the stage spawn brief fills this; the hook and CLI leave it empty.
    /// `rank_source` boosts nodes whose file is named here (A.23).
    pub dependency_paths: Vec<String>,
}

/// One scored candidate, before fusion.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    /// Stable chunk identifier.
    pub id: ChunkId,
    /// Channel whose list produced the candidate.
    pub channel: Channel,
    /// Pre-fusion relevance score.
    pub score: f32,
    /// Selection contributions that applied.
    pub reasons: Vec<SelectionReason>,
    /// Estimated chunk token cost.
    pub token_count: usize,
    /// Distinct query terms this candidate matched lexically. Feeds the hook's
    /// emit floor through `ContextItem::matched_term_count`.
    pub matched_term_count: usize,
}

/// One channel's ranking pass: its candidates, plus the query terms the corpus
/// dropped before scoring.
///
/// `dropped_terms` cannot be recovered from `candidates`: it names terms that
/// scored nothing anywhere, so no candidate mentions them — yet
/// [`crate::context::schema::ContextPack::dropped_terms`] has to report them.
/// It therefore rides out of the ranker beside the candidates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelRanking {
    /// Candidates in final order: score descending, id ascending.
    pub candidates: Vec<RankedCandidate>,
    /// Query terms dropped before scoring. Empty until A.2 lands.
    pub dropped_terms: Vec<String>,
}

/// Score the exact-match ladder for one chunk against `query`: explicit id,
/// exact path, exact symbol, linked-from, and stage-dependency boosts, in
/// that order. Returns the summed boost and the reasons that fired.
fn score_exact_match_ladder(
    query: &RankQuery,
    chunk: &KnowledgeChunk,
    explicit_chunks: &[&KnowledgeChunk],
    explicit_files: &[&PathBuf],
) -> (f32, Vec<SelectionReason>) {
    let mut score = 0.0;
    let mut reasons = Vec::new();
    if query.required_ids.iter().any(|id| id == &chunk.id) {
        score += BOOST_EXPLICIT_ID;
        reasons.push(SelectionReason::ExplicitId);
    }
    if chunk
        .source_paths
        .iter()
        .any(|path| contains_whole_term(&query.text, path))
    {
        score += BOOST_EXACT_PATH;
        reasons.push(SelectionReason::ExactPath);
    }
    if chunk
        .symbols
        .iter()
        .any(|symbol| contains_whole_term(&query.text, symbol))
    {
        score += BOOST_EXACT_SYMBOL;
        reasons.push(SelectionReason::ExactSymbol);
    }
    let links_to_explicit = chunk.links.iter().any(|(_, target)| {
        explicit_files
            .iter()
            .any(|file| link_target_matches(&chunk.file, target, file))
    });
    let linked_from_explicit = explicit_chunks.iter().any(|explicit| {
        explicit
            .links
            .iter()
            .any(|(_, target)| link_target_matches(&explicit.file, target, &chunk.file))
    });
    if links_to_explicit || linked_from_explicit {
        score += BOOST_LINKED_FROM;
        reasons.push(SelectionReason::LinkedFrom);
    }
    if query.stage_dependency_ids.iter().any(|id| id == &chunk.id) {
        score += BOOST_STAGE_DEPENDENCY;
        reasons.push(SelectionReason::StageDependency);
    }
    (score, reasons)
}

/// BM25 lexical score for the document at `index`, summed across query terms.
///
/// Returns the score and how many DISTINCT query terms matched the document.
/// The count is de-duplicated because `query_terms` may legitimately repeat a
/// term (a prompt saying "cache" twice tokenizes to two entries), and a repeat
/// is not extra evidence — the hook's emit floor counts *how much of the query*
/// an item covers, not how often the caller typed a word. The BM25 sum is
/// deliberately left alone: a repeated term contributing twice is the existing
/// weighting, and changing it is a ranking decision, not a counting one.
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
    let mut lexical_score = 0.0;
    let mut matched_terms: BTreeSet<&str> = BTreeSet::new();
    for term in query_terms {
        let document_frequency = document_frequencies[term];
        if document_frequency == 0 {
            continue;
        }
        let frequency = document_frequency as f32;
        let idf = (1.0 + (corpus_size - frequency + 0.5) / (frequency + 0.5)).ln();
        let matching_weights: Vec<f32> = documents[index]
            .iter()
            .filter(|(value, _)| value == term)
            .map(|(_, weight)| *weight)
            .collect();
        if matching_weights.is_empty() {
            continue;
        }
        let weighted_frequency: f32 = matching_weights.iter().sum();
        let normalization =
            BM25_K1 * (1.0 - BM25_B + BM25_B * lengths[index] as f32 / average_length);
        lexical_score +=
            idf * (weighted_frequency * (BM25_K1 + 1.0)) / (weighted_frequency + normalization);
        matched_terms.insert(term.as_str());
    }
    (lexical_score, matched_terms.len())
}

/// Query-dependent corpus statistics that do not depend on what a document
/// *is*: tokenized query terms, per-document weighted tokens and lengths, the
/// corpus average length, and per-term document frequencies.
///
/// Nothing here knows whether a document was built from a knowledge chunk or
/// from a source-graph node, which is the point: every channel builds its own
/// `(term, weight)` documents and then scores through the same statistics, so
/// the rankers cannot drift apart in how they weigh a term.
pub(crate) struct LexicalCorpus {
    /// Tokenized query terms, in query order.
    pub(crate) query_terms: Vec<String>,
    /// Weighted `(term, weight)` pairs per document, parallel to the input.
    pub(crate) documents: Vec<Vec<(String, f32)>>,
    /// Token count per document, parallel to `documents`.
    pub(crate) lengths: Vec<usize>,
    /// Mean document length; `0.0` for an empty corpus.
    pub(crate) average_length: f32,
    /// How many documents contain each query term.
    pub(crate) document_frequencies: BTreeMap<String, usize>,
    /// Query terms dropped before scoring. Empty until A.2 lands.
    pub(crate) dropped_terms: Vec<String>,
}

/// Assemble the loop-invariant statistics for one ranking pass.
///
/// Every query term gets a `document_frequencies` entry, including terms no
/// document contains at all: [`score_bm25`] indexes that map directly, so a
/// missing key panics instead of scoring zero.
///
/// `_raw_query` is the untokenized query text and `_config` the retrieval
/// tunables; both are threaded in now so A.2 can partition `query_terms` into
/// surviving and dropped without changing a single call site.
pub(crate) fn prepare_lexical(
    query_terms: &[String],
    documents: Vec<Vec<(String, f32)>>,
    // Wave 2 (A.1/A.2) reads this: backticked terms are never dropped.
    _raw_query: &str,
    // Wave 2 (A.1/A.2) reads this: `stop_df_ratio`, `min_query_token_len`.
    _config: &RetrievalConfig,
) -> LexicalCorpus {
    let lengths: Vec<usize> = documents.iter().map(Vec::len).collect();
    let average_length = if documents.is_empty() {
        0.0
    } else {
        lengths.iter().sum::<usize>() as f32 / documents.len() as f32
    };

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

    LexicalCorpus {
        query_terms: query_terms.to_vec(),
        documents,
        lengths,
        average_length,
        document_frequencies,
        dropped_terms: Vec::new(),
    }
}

/// The knowledge channel's corpus: the shared [`LexicalCorpus`] plus the two
/// chunk-specific sets the exact-match ladder needs — the chunks named by
/// `required_ids` and the files they live in.
struct Corpus<'a> {
    lexical: LexicalCorpus,
    explicit_chunks: Vec<&'a KnowledgeChunk>,
    explicit_files: Vec<&'a PathBuf>,
}

impl<'a> Corpus<'a> {
    fn prepare(query: &RankQuery, chunks: &'a [KnowledgeChunk], config: &RetrievalConfig) -> Self {
        let query_terms = tokenize(&query.text);
        let documents: Vec<Vec<(String, f32)>> = chunks.iter().map(field_tokens).collect();
        let explicit_chunks: Vec<&KnowledgeChunk> = chunks
            .iter()
            .filter(|chunk| query.required_ids.iter().any(|id| id == &chunk.id))
            .collect();
        let explicit_files: Vec<&PathBuf> =
            explicit_chunks.iter().map(|chunk| &chunk.file).collect();

        Self {
            lexical: prepare_lexical(&query_terms, documents, &query.text, config),
            explicit_chunks,
            explicit_files,
        }
    }
}

/// The one deterministic order for a ranked list: score descending, then
/// ascending id, so two runs over identical bytes agree completely.
fn by_score_then_id(left: &RankedCandidate, right: &RankedCandidate) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.id.cmp(&right.id))
}

/// Score one chunk, returning `None` when no reason fired — a chunk nothing in
/// the query pointed at is not a candidate at all.
///
/// The mirror of `rank_source`'s `score_node`, deliberately: the two channels
/// share the rung ladder, the BM25 statistics and the candidate type, and
/// keeping their per-document scorers the same shape is what makes a drift
/// between them visible in a diff.
fn score_chunk(
    query: &RankQuery,
    chunk: &KnowledgeChunk,
    channel: Channel,
    corpus: &Corpus<'_>,
    corpus_size: f32,
    index: usize,
) -> Option<RankedCandidate> {
    let (mut score, mut reasons) = score_exact_match_ladder(
        query,
        chunk,
        &corpus.explicit_chunks,
        &corpus.explicit_files,
    );
    let (lexical_score, matched_term_count) = score_bm25(
        &corpus.lexical.query_terms,
        &corpus.lexical.document_frequencies,
        &corpus.lexical.documents,
        &corpus.lexical.lengths,
        corpus.lexical.average_length,
        corpus_size,
        index,
    );
    if matched_term_count > 0 {
        reasons.push(SelectionReason::Lexical);
        score += lexical_score;
    }
    if reasons.is_empty() {
        return None;
    }
    Some(RankedCandidate {
        id: ChunkId::from(chunk.id.as_str()),
        channel,
        score,
        reasons,
        token_count: chunk.estimated_tokens,
        matched_term_count,
    })
}

/// Score every chunk against the query for one channel, reporting the corpus
/// diagnostics alongside the candidates.
///
/// Results descend by score and use ascending id as a deterministic
/// tie-breaker. This is the form [`crate::context::retrieve`] calls, because
/// the pack has to report the dropped terms; [`rank`] is the same pass without
/// them.
pub fn rank_channel(
    query: &RankQuery,
    chunks: &[KnowledgeChunk],
    channel: Channel,
    config: &RetrievalConfig,
) -> ChannelRanking {
    if chunks.is_empty() {
        return ChannelRanking::default();
    }

    let corpus = Corpus::prepare(query, chunks, config);
    let corpus_size = chunks.len() as f32;
    let mut candidates = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if let Some(candidate) = score_chunk(query, chunk, channel, &corpus, corpus_size, index) {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(by_score_then_id);
    ChannelRanking {
        candidates,
        dropped_terms: corpus.lexical.dropped_terms,
    }
}

/// Score every chunk against the query for one channel.
///
/// Results descend by score and use ascending id as a deterministic
/// tie-breaker. [`rank_channel`] without the corpus diagnostics, for callers
/// that only want the ordering.
pub fn rank(
    query: &RankQuery,
    chunks: &[KnowledgeChunk],
    channel: Channel,
    config: &RetrievalConfig,
) -> Vec<RankedCandidate> {
    rank_channel(query, chunks, channel, config).candidates
}
