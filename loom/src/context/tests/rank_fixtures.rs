//! Shared hand-built fixtures for the knowledge-ranker tests, mirroring
//! `source_fixtures.rs`'s style for the source channel.
//!
//! `pub(super)` makes every item here visible to any sibling module under
//! `context::tests` via `use super::rank_fixtures::...`.

use crate::context::config::RetrievalConfig;
use crate::context::rank::{rank, RankQuery, RankedCandidate};
use crate::context::schema::{Channel, KnowledgeChunk, LifecycleState};
use std::path::PathBuf;

/// A heading that makes a fixture chunk a section rather than a stub
/// (`rank/candidacy.rs`) while adding no term to it: punctuation tokenizes to
/// nothing, so the BM25 arithmetic the ranker tests pin is still measured over
/// the body alone.
pub(super) const TERMLESS_HEADING: &str = "—";

/// Build a `KnowledgeChunk` with every field explicit but the interesting ones
/// (`symbols`, `source_paths`, `links`) left for the caller to set.
pub(super) fn chunk(id: &str, body: &str, tokens: usize) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.to_string(),
        file: PathBuf::from(format!("{id}.md")),
        anchor: String::new(),
        heading: TERMLESS_HEADING.to_string(),
        body: body.to_string(),
        content_hash: String::new(),
        estimated_tokens: tokens,
        aliases: Vec::new(),
        category: None,
        source_paths: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
        state: LifecycleState::Active,
    }
}

/// Rank `chunks` against a plain text query on [`Channel::Knowledge`] with the
/// default config — the shape almost every ranker test needs.
pub(super) fn rank_text(text: &str, chunks: &[KnowledgeChunk]) -> Vec<RankedCandidate> {
    rank(
        &RankQuery {
            text: text.to_string(),
            ..RankQuery::default()
        },
        chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    )
}
