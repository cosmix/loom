//! Deterministic BM25 ranking for knowledge chunks, plus the document-agnostic
//! corpus machinery (`LexicalCorpus`, `prepare_lexical`, `score_bm25`)
//! that [`crate::context::rank_source()`] scores source-graph nodes through.

use crate::context::lexical::{contains_whole_term, field_tokens, link_target_matches};
use crate::context::schema::{Channel, ChunkId, KnowledgeChunk, SelectionReason};
use std::cmp::Ordering;
use std::collections::BTreeMap;
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
/// Returns the score and whether any term matched at all.
#[allow(clippy::too_many_arguments)]
pub(crate) fn score_bm25(
    query_terms: &[String],
    document_frequencies: &BTreeMap<String, usize>,
    documents: &[Vec<(String, f32)>],
    lengths: &[usize],
    average_length: f32,
    corpus_size: f32,
    index: usize,
) -> (f32, bool) {
    let mut lexical_score = 0.0;
    let mut has_lexical_match = false;
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
        has_lexical_match = true;
    }
    (lexical_score, has_lexical_match)
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
}

/// Assemble the loop-invariant statistics for one ranking pass.
///
/// Every query term gets a `document_frequencies` entry, including terms no
/// document contains at all: [`score_bm25`] indexes that map directly, so a
/// missing key panics instead of scoring zero.
pub(crate) fn prepare_lexical(
    query_terms: &[String],
    documents: Vec<Vec<(String, f32)>>,
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
    fn prepare(query: &RankQuery, chunks: &'a [KnowledgeChunk]) -> Self {
        let query_terms = tokenize(&query.text);
        let documents: Vec<Vec<(String, f32)>> = chunks.iter().map(field_tokens).collect();
        let explicit_chunks: Vec<&KnowledgeChunk> = chunks
            .iter()
            .filter(|chunk| query.required_ids.iter().any(|id| id == &chunk.id))
            .collect();
        let explicit_files: Vec<&PathBuf> =
            explicit_chunks.iter().map(|chunk| &chunk.file).collect();

        Self {
            lexical: prepare_lexical(&query_terms, documents),
            explicit_chunks,
            explicit_files,
        }
    }
}

/// Score every chunk against the query for one channel.
///
/// Results descend by score and use ascending id as a deterministic tie-breaker.
pub fn rank(
    query: &RankQuery,
    chunks: &[KnowledgeChunk],
    channel: Channel,
) -> Vec<RankedCandidate> {
    if chunks.is_empty() {
        return Vec::new();
    }

    let corpus = Corpus::prepare(query, chunks);
    let corpus_size = chunks.len() as f32;
    let mut candidates = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let (mut score, mut reasons) = score_exact_match_ladder(
            query,
            chunk,
            &corpus.explicit_chunks,
            &corpus.explicit_files,
        );
        let (lexical_score, has_lexical_match) = score_bm25(
            &corpus.lexical.query_terms,
            &corpus.lexical.document_frequencies,
            &corpus.lexical.documents,
            &corpus.lexical.lengths,
            corpus.lexical.average_length,
            corpus_size,
            index,
        );
        if has_lexical_match {
            reasons.push(SelectionReason::Lexical);
            score += lexical_score;
        }
        if !reasons.is_empty() {
            candidates.push(RankedCandidate {
                id: ChunkId::from(chunk.id.as_str()),
                channel,
                score,
                reasons,
                token_count: chunk.estimated_tokens,
            });
        }
    }
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    candidates
}
