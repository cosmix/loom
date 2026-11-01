//! The knowledge channel's exact-match ladder: the rungs a curated chunk can
//! earn before BM25 is added on top.
//!
//! These rungs inspect `KnowledgeChunk` fields; the source ranker has its own
//! ladder, while both channels share document-agnostic corpus statistics.

use super::{
    RankQuery, RungScore, BOOST_EXACT_PATH, BOOST_EXACT_SYMBOL, BOOST_EXPLICIT_ID,
    BOOST_LINKED_FROM, BOOST_STAGE_DEPENDENCY,
};
use crate::context::lexical::{contains_whole_term, link_target_matches, ExactGate};
use crate::context::schema::{KnowledgeChunk, SelectionReason};
use std::path::PathBuf;

/// Score the exact-match ladder for one chunk against `query`: explicit id,
/// exact path, exact symbol, linked-from, and stage-dependency boosts, in
/// that order.
///
/// `ExactPath` here is always the FULL-relative-path arm — a chunk's
/// `source_paths` hold whole paths, never bare stems — so it is ungated: a path
/// written out in a prompt is deliberate. `ExactSymbol` goes through `gate`,
/// because a chunk symbol is exactly as likely to be an ordinary English word
/// as a source node's is.
pub(super) fn score_exact_match_ladder(
    query: &RankQuery,
    chunk: &KnowledgeChunk,
    explicit_chunks: &[&KnowledgeChunk],
    explicit_files: &[&PathBuf],
    gate: &ExactGate<'_>,
) -> RungScore {
    let mut rungs = RungScore::default();
    if query.required_ids.iter().any(|id| id == &chunk.id) {
        rungs.award(BOOST_EXPLICIT_ID, SelectionReason::ExplicitId);
    }
    if chunk
        .source_paths
        .iter()
        .any(|path| contains_whole_term(&query.text, path))
    {
        rungs.award(BOOST_EXACT_PATH, SelectionReason::ExactPath);
    }
    if let Some(evidence) = chunk.symbols.iter().find_map(|symbol| gate.admits(symbol)) {
        rungs.award_matched(BOOST_EXACT_SYMBOL, SelectionReason::ExactSymbol, &evidence);
    }
    if links_explicit(chunk, explicit_chunks, explicit_files) {
        rungs.award(BOOST_LINKED_FROM, SelectionReason::LinkedFrom);
    }
    if query.stage_dependency_ids.iter().any(|id| id == &chunk.id) {
        rungs.award(BOOST_STAGE_DEPENDENCY, SelectionReason::StageDependency);
    }
    rungs
}

/// True when `chunk` links to an explicitly required chunk's file, or an
/// explicitly required chunk links to `chunk`'s file. Link adjacency counts in
/// either direction: the neighbour of a chunk the caller demanded is context
/// for it whichever way the arrow points.
fn links_explicit(
    chunk: &KnowledgeChunk,
    explicit_chunks: &[&KnowledgeChunk],
    explicit_files: &[&PathBuf],
) -> bool {
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
    links_to_explicit || linked_from_explicit
}
