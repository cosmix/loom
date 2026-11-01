//! Deterministic BM25 ranking for knowledge chunks.
//!
//! The document-agnostic corpus machinery (`LexicalCorpus`,
//! `prepare_lexical_cached`, `score_bm25`) lives in `corpus`, the exact-rung
//! accumulator in `rungs`, and this channel's chunk-specific exact-match
//! ladder in `ladder`. The first two are re-exported here because
//! [`crate::context::rank_source()`] scores source-graph nodes through exactly
//! the same statistics and the same rung ladder; `ladder` is not, because it
//! reads `KnowledgeChunk` fields a source node does not have.

mod candidacy;
mod candidate;
pub(crate) mod corpus;
mod ladder;
mod rungs;

pub use candidate::{neighbor_explanation, EdgeDirection, NeighborVia, RankQuery, RankedCandidate};
pub(crate) use corpus::{prepare_lexical_cached, LexicalCorpus};
pub(crate) use rungs::RungScore;

// The scan oracle, re-exported for `context/tests/rank_stopwords.rs`, which
// pins BM25 one input at a time against it. Production reaches the same
// arithmetic through `LexicalCorpus::score`, which is what picks between the
// scanned and the indexed representation (`rank/corpus.rs:164`), so nothing
// outside the tests names `score_bm25` any more — hence the `cfg`, which keeps
// a non-test build from warning on an unused re-export. `doc` rides along
// because `lexical_index.rs:22` links this path when arguing that the scan is
// not dead code, and that argument should not go dark in the rendered docs.
#[cfg(any(test, doc))]
pub(crate) use corpus::score_bm25;

use crate::context::config::RetrievalConfig;
use crate::context::lexical::{field_tokens, ExactGate};
use crate::context::lexical_index::LexicalCache;
use crate::context::schema::{Channel, ChunkId, KnowledgeChunk, SelectionReason};
use crate::fs::knowledge::catalog::prose::PROSE_ID_PREFIX;
use ladder::score_exact_match_ladder;
use std::cmp::Ordering;
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

/// Demote already admitted prose without inflating curated scores across channels.
/// The caller clamps at zero before fusion so normalized scores remain ordered.
fn prose_demotion(chunk: &KnowledgeChunk, config: &RetrievalConfig) -> f32 {
    if chunk.id.starts_with(PROSE_ID_PREFIX) {
        config.knowledge_curated_prior
    } else {
        0.0
    }
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
    /// Query terms dropped as corpus-ubiquitous or too short to discriminate.
    pub dropped_terms: Vec<String>,
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
    /// Assemble the corpus, reading the persistent lexical index (A.13) when
    /// `cache` is `Some` and warm, and building it when it is not.
    ///
    /// The documents go in as a CLOSURE rather than a `Vec` because skipping
    /// their construction IS the optimization: `field_tokens` runs over every
    /// chunk in the catalog — 656 of them in this repository — on every prompt,
    /// inside a hook with a five-second ceiling. `doc_ids` are the chunk ids in
    /// corpus order, and they are what proves a warm file describes THIS
    /// catalog rather than a same-revision-different-chunks one.
    fn prepare(
        query: &RankQuery,
        chunks: &'a [KnowledgeChunk],
        config: &RetrievalConfig,
        cache: Option<&LexicalCache>,
    ) -> Self {
        let query_terms = tokenize(&query.text);
        let doc_ids: Vec<&str> = chunks.iter().map(|chunk| chunk.id.as_str()).collect();
        let explicit_chunks: Vec<&KnowledgeChunk> = chunks
            .iter()
            .filter(|chunk| query.required_ids.iter().any(|id| id == &chunk.id))
            .collect();
        let explicit_files: Vec<&PathBuf> =
            explicit_chunks.iter().map(|chunk| &chunk.file).collect();

        Self {
            lexical: prepare_lexical_cached(
                &query_terms,
                &doc_ids,
                || chunks.iter().map(field_tokens).collect(),
                &query.text,
                config,
                cache,
            ),
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
/// the query pointed at is not a candidate at all. With stopwording in place
/// that is a real filter rather than a formality: a chunk whose only overlap
/// with the prompt was "the" now matches no surviving term, earns no reason,
/// and never becomes a candidate to be counted as `omitted`.
///
/// The mirror of `rank_source`'s `score_node`, deliberately: the two channels
/// share the rung ladder, the BM25 statistics and the candidate type, and
/// keeping their per-document scorers the same shape is what makes a drift
/// between them visible in a diff.
#[allow(clippy::too_many_arguments)]
fn score_chunk(
    query: &RankQuery,
    chunk: &KnowledgeChunk,
    channel: Channel,
    corpus: &Corpus<'_>,
    gate: &ExactGate<'_>,
    corpus_size: f32,
    index: usize,
) -> Option<RankedCandidate> {
    let mut rungs = score_exact_match_ladder(
        query,
        chunk,
        &corpus.explicit_chunks,
        &corpus.explicit_files,
        gate,
    );
    let (lexical_score, matched_terms) = corpus.lexical.score(corpus_size, index);
    if matched_terms > 0 {
        rungs.reasons.push(SelectionReason::Lexical);
        rungs.score += lexical_score;
    }
    if rungs.is_empty() {
        return None;
    }
    let matched_term_count = candidacy::named_terms(&corpus.lexical, chunk);
    // Read before `reasons` is moved out of `rungs` below.
    let confidence_ceiling = rungs.confidence_ceiling();
    Some(RankedCandidate {
        id: ChunkId::from(chunk.id.as_str()),
        channel,
        score: rungs.score,
        reasons: rungs.reasons,
        token_count: chunk.estimated_tokens,
        matched_term_count,
        confidence_ceiling,
        via: None,
    })
}

/// Score every chunk against the query for one channel, reporting the corpus
/// diagnostics alongside the candidates.
///
/// Results descend by score and use ascending id as a deterministic
/// tie-breaker. This is the shape [`crate::context::retrieve`] consumes,
/// because the pack has to report the dropped terms; [`rank`] is the same pass
/// without them.
///
/// Scores over a freshly tokenized corpus — the scan path, and the oracle the
/// persistent index is checked against. [`rank_channel_cached`] is the same
/// pass with an index behind it, and is what retrieval itself reaches for.
pub fn rank_channel(
    query: &RankQuery,
    chunks: &[KnowledgeChunk],
    channel: Channel,
    config: &RetrievalConfig,
) -> ChannelRanking {
    rank_channel_cached(query, chunks, channel, config, None)
}

/// [`rank_channel`], reading and maintaining the persistent lexical index
/// (A.13) when `cache` is given.
///
/// The `Option` is what keeps the scan reachable: a caller with no context
/// cache root — and every existing test — passes `None` and gets the full
/// scan, so the fallback path stays exercised rather than becoming code that
/// only runs after a cache is deleted. Mirrors
/// [`crate::context::rank_source::rank_source_channel_cached`] deliberately;
/// the two channels' entry points are kept the same shape so a drift between
/// them shows up in a diff.
pub fn rank_channel_cached(
    query: &RankQuery,
    chunks: &[KnowledgeChunk],
    channel: Channel,
    config: &RetrievalConfig,
    cache: Option<&LexicalCache>,
) -> ChannelRanking {
    if chunks.is_empty() {
        return ChannelRanking::default();
    }

    // Order is load-bearing: the gate's `rare` test reads the corpus document
    // frequencies, so the corpus has to exist before any rung is scored.
    let corpus = Corpus::prepare(query, chunks, config, cache);
    let gate = ExactGate::new(
        &query.text,
        &corpus.lexical.document_frequencies,
        config.df_ident_max,
    );
    let corpus_size = chunks.len() as f32;
    let mut candidates = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if !candidacy::admits(query, chunk) {
            continue;
        }
        if let Some(mut candidate) =
            score_chunk(query, chunk, channel, &corpus, &gate, corpus_size, index)
        {
            candidate.score = (candidate.score - prose_demotion(chunk, config)).max(0.0);
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
