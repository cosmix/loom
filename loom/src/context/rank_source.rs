//! Deterministic ranking of derived source-graph nodes for
//! [`Channel::Source`].
//!
//! The knowledge ranker in [`crate::context::rank`] scores curated prose; this
//! one scores the symbols extracted into the source graph, reusing the same
//! exact-match rungs, the same BM25 statistics and the same candidate type so
//! the two lists are directly comparable when [`crate::context::fuse`] merges
//! them by reciprocal rank.
//!
//! ## Note for whoever next touches `fuse`
//!
//! Fusion keys its accumulator by [`ChunkId`] *across* channels. Knowledge ids
//! are `<path>#<heading>#<occurrence>` while source ids are
//! `<path>#<kind>:<scope>`, over disjoint path spaces, so a collision is not
//! reachable today — but if one ever occurred, `fuse` would merge the two
//! entries and adopt the
//! better-ranked channel, after which [`crate::context::pack`]'s
//! channel-dispatch would consult the wrong map and silently drop the item. Do
//! not "simplify" that dispatch into trying both maps: it would hide the
//! collision instead of surfacing it.

use crate::context::graph_store::ResolvedGraph;
use crate::context::lexical::{contains_whole_term, WEIGHT_BODY, WEIGHT_SYMBOLS};
use crate::context::rank::{
    prepare_lexical, score_bm25, tokenize, LexicalCorpus, RankQuery, RankedCandidate,
    BOOST_EXACT_PATH, BOOST_EXACT_SYMBOL, BOOST_EXPLICIT_ID,
};
use crate::context::schema::{
    Channel, ChunkId, FileCoverage, SelectionReason, SourceNode, SourceNodeKind,
};
use std::cmp::Ordering;
use std::path::Path;

/// Most source candidates one ranking pass hands to fusion.
///
/// [`crate::context::fuse`] is pure reciprocal-rank fusion, so a rank-1 source
/// node contributes exactly what a rank-1 knowledge chunk contributes, and
/// there is no per-channel budget anywhere in the pipeline. Packing then walks
/// the fused list in order taking whole units until the budget is spent (see
/// [`crate::context::pack`]). A source item costs ~20-30 tokens against a knowledge
/// chunk's few hundred, so an unbounded source list does not crowd out prose by
/// token volume — it crowds it out by *slot*, one alternating rank at a time.
/// 60 keeps the source channel able to answer a symbol query fully while
/// leaving the fused head dominated by prose: source facts are a pointer into
/// the code, prose is the thing that cannot be re-derived from the code.
const MAX_SOURCE_CANDIDATES: usize = 60;

/// A scored node paired with the positional key its ties break on.
struct ScoredNode<'a> {
    candidate: RankedCandidate,
    order: (&'a Path, usize),
}

/// Score every symbol node of `graph` against the query for
/// [`Channel::Source`].
///
/// Results descend by score and break ties by `(path, line_start)` so two runs
/// over identical bytes produce an identical order.
pub fn rank_source(query: &RankQuery, graph: &ResolvedGraph) -> Vec<RankedCandidate> {
    // Whole-file nodes carry no signature and no scope, so they can only ever
    // match on their path — one per file, crowding out the symbols inside it.
    let nodes: Vec<&SourceNode> = graph
        .nodes()
        .filter(|node| !matches!(node.kind, SourceNodeKind::File))
        .collect();
    if nodes.is_empty() {
        return Vec::new();
    }

    let query_terms = tokenize(&query.text);
    let documents: Vec<Vec<(String, f32)>> = nodes.iter().copied().map(node_document).collect();
    let corpus = prepare_lexical(&query_terms, documents);
    let corpus_size = nodes.len() as f32;

    let mut scored: Vec<ScoredNode> = nodes
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, node)| {
            score_node(query, node, &corpus, corpus_size, index).map(|candidate| ScoredNode {
                candidate,
                order: (node.path.as_path(), node.span.line_start),
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.order.cmp(&b.order))
    });
    scored.truncate(MAX_SOURCE_CANDIDATES);
    scored.into_iter().map(|scored| scored.candidate).collect()
}

/// Score one node, returning `None` when no reason fired — a node nothing in
/// the query pointed at is not a candidate at all.
fn score_node(
    query: &RankQuery,
    node: &SourceNode,
    corpus: &LexicalCorpus,
    corpus_size: f32,
    index: usize,
) -> Option<RankedCandidate> {
    let (mut score, mut reasons) = withhold_partial_coverage(node, score_exact_rungs(query, node));
    let (lexical_score, has_lexical_match) = score_bm25(
        &corpus.query_terms,
        &corpus.document_frequencies,
        &corpus.documents,
        &corpus.lengths,
        corpus.average_length,
        corpus_size,
        index,
    );
    if has_lexical_match {
        reasons.push(SelectionReason::Lexical);
        score += lexical_score;
    }
    if reasons.is_empty() {
        return None;
    }
    Some(RankedCandidate {
        id: ChunkId::from(node.id.as_str()),
        channel: Channel::Source,
        score,
        reasons,
        token_count: estimate_node_tokens(node),
    })
}

/// Exact-match rungs for one node: explicit id, exact path, exact symbol.
///
/// The rungs search the *raw* query text for the candidate's own string rather
/// than comparing tokens. [`tokenize`] splits on every non-identifier
/// character, so `Foo::Bar` becomes `foo, bar` and `src/context/pack.rs`
/// becomes `src, context, pack, rs` — token equality would match neither a
/// CamelCase symbol nor a whole path. Tokens feed BM25 and nothing else.
fn score_exact_rungs(query: &RankQuery, node: &SourceNode) -> (f32, Vec<SelectionReason>) {
    let mut score = 0.0;
    let mut reasons = Vec::new();
    if query.required_ids.iter().any(|id| id == &node.id) {
        score += BOOST_EXPLICIT_ID;
        reasons.push(SelectionReason::ExplicitId);
    }
    if matches_path(&query.text, node) {
        score += BOOST_EXACT_PATH;
        reasons.push(SelectionReason::ExactPath);
    }
    if node
        .scope
        .last()
        .is_some_and(|terminal| contains_whole_term(&query.text, terminal))
    {
        score += BOOST_EXACT_SYMBOL;
        reasons.push(SelectionReason::ExactSymbol);
    }
    (score, reasons)
}

/// Withhold every high-confidence rung from a node whose file was not fully
/// extracted, leaving it to stand on whatever lexical score it earns.
///
/// [`crate::context::schema::Confidence::from_reasons`] classifies `High` from
/// *any* of `ExplicitId`, `ExactPath` or `ExactSymbol` — one `matches!` over
/// all three — so the guard has to drop all three together. Withholding only
/// the two match rungs while still emitting `ExplicitId` would leave the
/// invariant violated by the single most likely query shape: a caller passing
/// `--require-id` for a node in a partially-extracted file. A required id that
/// loses its rung is still ranked and can still appear; it simply cannot claim
/// that the extraction behind it was complete.
fn withhold_partial_coverage(
    node: &SourceNode,
    scored: (f32, Vec<SelectionReason>),
) -> (f32, Vec<SelectionReason>) {
    if matches!(node.coverage, FileCoverage::Full) {
        return scored;
    }
    // Every rung score_exact_rungs awards is high-tier, so dropping them all
    // leaves no boost and no reason behind.
    (0.0, Vec::new())
}

/// Build one node's BM25 document from the text that identifies it: its scope
/// segments carry the symbol name and its owners, the signature the types and
/// parameters around them.
fn node_document(node: &SourceNode) -> Vec<(String, f32)> {
    let mut terms: Vec<(String, f32)> = node
        .scope
        .iter()
        .flat_map(|segment| tokenize(segment))
        .map(|term| (term, WEIGHT_SYMBOLS))
        .collect();
    terms.extend(
        tokenize(&node.signature)
            .into_iter()
            .map(|term| (term, WEIGHT_BODY)),
    );
    terms
}

/// True when the query names the node's file, spelled either as a path or as a
/// bare stem — `src/context/pack.rs` and `pack` must both match.
fn matches_path(query_text: &str, node: &SourceNode) -> bool {
    if contains_whole_term(query_text, &node.path.display().to_string()) {
        return true;
    }
    node.path
        .file_stem()
        .is_some_and(|stem| contains_whole_term(query_text, &stem.to_string_lossy()))
}

/// Estimated token cost of rendering one source node.
///
/// A source item is a signature and a pointer, a few lines at most. The
/// constant covers the pointer: under-estimating here overfills the pack, so
/// the estimate deliberately rounds up.
fn estimate_node_tokens(node: &SourceNode) -> usize {
    node.signature.len() / 4 + 16
}
