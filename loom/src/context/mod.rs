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
//! [`rank`] scores each channel independently, [`fuse`] merges the per-channel
//! rank lists by reciprocal rank fusion, and [`pack`] walks the fused list in
//! order taking whole chunks until the budget is spent. The packer never exceeds
//! its budget and always reports what it left out.
//!
//! ## Shadow mode
//!
//! Nothing here changes what an agent sees. Selection is reachable only through
//! the `loom knowledge context` / `status` / `sync` commands; signal generation
//! is untouched.

pub mod fingerprint;
pub mod fuse;
pub mod ingest;
mod lexical;
pub mod pack;
pub mod rank;
pub mod refresh;
pub mod schema;
pub mod source_graph;
pub mod store;

pub use schema::{
    estimate_tokens, Channel, ChunkId, Confidence, ContextItem, ContextPack, Coverage, Freshness,
    ItemKind, KnowledgeChunk, LifecycleState, OmissionSummary, SelectionReason, SourceEdge,
    SourceEdgeKind, SourceNode, SourcePointer, BYTES_PER_TOKEN_ESTIMATE,
};

#[cfg(test)]
mod tests;
