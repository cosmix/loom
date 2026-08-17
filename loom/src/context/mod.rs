//! Deterministic, model-free, network-free context retrieval over the curated
//! knowledge hierarchy.
//!
//! The subsystem answers one question — "which curated prose is worth spending
//! N tokens on for this query?" — and answers it the same way every time. There
//! is no embedding model, no network call, and no randomness anywhere below this
//! module; a [`schema::ContextPack`] is a pure function of the bytes on disk and
//! the query string.
//!
//! ## Pipeline
//!
//! ```text
//! knowledge/*.md ──chunker──> KnowledgeChunk ──catalog──> Catalog (revision)
//!        │                                                    │
//!    fingerprint ──> Freshness                             ingest
//!        │                                                    │
//!        └────────────> store (.loom/cache/context-v1/) <─────┘
//!                              │
//!                    rank ──> fuse ──> pack ──> ContextPack
//! ```
//!
//! [`rank`] scores each requested channel independently, [`fuse`] merges the
//! per-channel rank lists by reciprocal rank fusion, and [`pack`] walks the
//! fused list in order taking whole chunks until the budget is spent. The
//! packer never exceeds its budget and always reports what it left out.
//!
//! Only the knowledge-chunk channel actually contributes candidates today.
//! [`schema::Channel::Source`] is a real variant and [`source_graph`] builds a
//! real graph of source nodes and edges, but the two are not yet connected:
//! [`rank`] still only accepts `&[KnowledgeChunk]`, so ranking the source
//! channel over the same catalog would double-count the knowledge chunks.
//! `rank_channels` in [`retrieve`] therefore ranks `Source` over an empty
//! slice — the channel is present in the API and consumed by nothing; bridging
//! the graph's nodes into the ranker is separate, unbuilt work.
//!
//! ## One entry point
//!
//! [`retrieve::retrieve_for_stage`] runs that whole pipeline and is the only way
//! in: the `loom knowledge context` command, signal generation and the prompt
//! hook all call it, so a brief rendered at spawn time and a brief pulled by
//! hand are built the same way. [`delivery`] then records what a recipient was
//! actually given, so the next retrieval in the same generation can skip it.

pub mod coverage;
pub mod delivery;
pub mod extract;
pub mod fingerprint;
pub mod freshness;
pub mod fuse;
pub mod graph_store;
pub mod ingest;
mod lexical;
pub mod pack;
pub mod rank;
pub mod refresh;
pub mod resolve;
pub mod retrieve;
pub mod schema;
pub mod source_graph;
pub mod store;
pub(crate) mod untrusted;

pub use coverage::CoverageReport;
pub use resolve::{impact, resolve_graph, ImpactHit, ResolutionStats, SymbolIndex};
pub use retrieve::{retrieve_for_stage, StageQuery};

pub use schema::{
    estimate_tokens, Channel, ChunkId, Confidence, ContextItem, ContextPack, Coverage,
    EdgeProvenance, FileCoverage, Freshness, ItemKind, KnowledgeChunk, LifecycleState,
    NodeLanguage, OmissionSummary, SelectionReason, SourceEdge, SourceEdgeKind, SourceNode,
    SourceNodeKind, SourcePointer, Span, BYTES_PER_TOKEN_ESTIMATE, EXCERPT_MAX_TOKENS,
    EXCERPT_TRUNCATION_MARKER,
};

#[cfg(test)]
mod tests;
