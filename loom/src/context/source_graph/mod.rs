//! Types for the derived source graph — the second retrieval channel.
//!
//! The source-graph extractors populate these; `crate::context` retrieval only
//! ever reads them. They live here rather than in [`crate::context::schema`]
//! because the graph is a distinct domain from the knowledge corpus.
//!
//! [`crate::context::schema`] re-exports the public names, so callers may reach
//! them through either path.
//!
//! ## The honesty contract
//!
//! **This graph is never claimed to be complete.** Every [`SourceEdge`] carries
//! an [`EdgeProvenance`] and an explicit confidence, and a call whose target
//! cannot be resolved is emitted as an *inferred* edge or as
//! [`UNRESOLVED_TARGET`] — never as an authoritative parser edge. Consumers that
//! render or traverse the graph must surface that confidence rather than
//! flattening it away.

mod edge;
mod node;

pub use edge::{EdgeProvenance, SourceEdge, SourceEdgeKind};
pub use node::{FileCoverage, NodeLanguage, SourceNode, SourceNodeKind, Span};

/// Placeholder [`SourceEdge::to`] for a call or import whose target could not be
/// resolved. Distinct from a resolved id so a traversal can report "unresolved"
/// instead of silently dropping the edge or inventing a destination.
pub const UNRESOLVED_TARGET: &str = "<unresolved>";

/// Confidence ceiling for an edge whose target was not resolved within the file.
/// Extractors must not exceed it for [`EdgeProvenance::Inferred`] edges.
///
/// An extractor sees exactly one file, so it cannot tell a name it does not
/// recognize from a name that is defined next door. Half confidence is the
/// most that view can honestly support.
pub const MAX_INFERRED_CONFIDENCE: f32 = 0.5;

/// Confidence ceiling for an inferred edge that whole-graph resolution matched
/// to exactly one definition.
///
/// Deliberately below `1.0`: cross-file uniqueness is real evidence an
/// extractor never had, but a unique *name* match is still not a parse. Two
/// unrelated crates can define one name, and a graph that omits a file omits
/// its definitions too — so "the only match I can see" is not "the only match".
/// Reserving `1.0` for [`EdgeProvenance::Parser`] keeps the strongest claim
/// attached to the only evidence that actually proves it.
pub const MAX_RESOLVED_INFERRED_CONFIDENCE: f32 = 0.9;

/// Files larger than this are recorded at file level only, never parsed.
///
/// Parsing is linear in file size but the query walk is not free, and a
/// multi-megabyte generated file contributes almost nothing to retrieval. The
/// cap keeps a pathological input from stalling a refresh.
pub const MAX_EXTRACTED_FILE_BYTES: usize = 512 * 1024;

/// Build the canonical id for a file node: the relative path, forward-slashed.
pub fn file_node_id(path: &std::path::Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Build the canonical id for a symbol node: `<relative-path>#<kind>:<scope-joined>`.
///
/// `scope` is outermost-first and joined with `::` regardless of language, so
/// ids are comparable across extractors. A symbol with an empty scope is
/// invalid — callers must pass at least the symbol's own name.
///
/// **The kind is part of the id because scope alone is not unique.** Rust's
/// `struct Widget` and `impl Widget` share a name, as do a TypeScript
/// `interface Foo` and a `const Foo`, and a Rust brace-struct and a same-named
/// function occupy different namespaces legally. Keying on scope alone let an
/// implementation node silently shadow the type it implements — collapsing two
/// distinct nodes into one and making their `Contains` edges
/// indistinguishable, so a traversal could not tell which parent a method
/// belonged to.
pub fn node_id(path: &std::path::Path, kind: SourceNodeKind, scope: &[String]) -> String {
    format!(
        "{}#{}:{}",
        file_node_id(path),
        kind.as_str(),
        scope.join("::")
    )
}

/// `sha256:<hex>` over arbitrary bytes — the one definition of a `body_hash`.
pub fn body_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests;
