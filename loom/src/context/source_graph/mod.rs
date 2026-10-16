//! Types for the derived source graph.
//!
//! The source-graph extractors populate these, and `crate::commands::map`
//! consumes them (via `crate::context::graph_store`) to render `loom map`.
//! The retrieval pipeline reads them too, through
//! `crate::context::rank_source`, so `Channel::Source` candidates are ranked
//! over the same nodes `loom map` renders. They live here rather than in
//! [`crate::context::schema`] because the graph is a distinct domain from the
//! knowledge corpus.
//!
//! [`crate::context::schema`] re-exports the public names, so callers may reach
//! them through either path.
//!
//! ## The honesty contract
//!
//! **This graph is never claimed to be complete.** Every [`SourceEdge`] carries
//! an [`EdgeProvenance`] and an explicit confidence, and a call whose target
//! cannot be resolved is emitted as a `Syntax` edge to [`UNRESOLVED_TARGET`] —
//! never as a bound edge. Only containment is certain. Consumers that render or
//! traverse the graph must surface that confidence rather than flattening it
//! away.

mod edge;
mod imports;
mod node;
pub(crate) mod read;

pub use edge::{site_id, EdgeProvenance, SourceEdge, SourceEdgeKind};
pub use imports::ImportBinding;
pub use node::{FileCoverage, NodeLanguage, SourceNode, SourceNodeKind, Span};

/// Placeholder [`SourceEdge::to`] for a call or import whose target could not be
/// resolved. Distinct from a resolved id so a traversal can report "unresolved"
/// instead of silently dropping the edge or inventing a destination.
pub const UNRESOLVED_TARGET: &str = "<unresolved>";

/// Version of the persisted graph layer format. A layer written under another
/// version is never current and never a reuse source, so an old cache is
/// rebuilt instead of read.
pub const GRAPH_SCHEMA_VERSION: u32 = 3;

/// Confidence of a [`EdgeProvenance::Structural`] edge. Numeric confidence is
/// an evidence ranking, not a calibrated probability; only structural (and the
/// reserved compiler) edges may carry `1.0`.
pub const STRUCTURAL_CONFIDENCE: f32 = 1.0;

/// Confidence of a [`EdgeProvenance::Receiver`] edge: a `self`/`this` call bound
/// to a member of the enclosing type. An evidence ranking, not a probability.
pub const RECEIVER_CONFIDENCE: f32 = 0.85;

/// Confidence of an [`EdgeProvenance::Import`] edge: bound through an import,
/// alias, qualified path, or package scope. An evidence ranking, not a
/// probability.
pub const IMPORT_CONFIDENCE: f32 = 0.85;

/// Confidence of a [`EdgeProvenance::LocalName`] edge: a same-file spelling with
/// exactly one in-scope definition. An evidence ranking, not a probability.
pub const LOCAL_NAME_CONFIDENCE: f32 = 0.8;

/// Confidence of a [`EdgeProvenance::UniqueName`] edge: the only same-family
/// definition of the name in the graph. Deliberately low: two unrelated crates
/// can define one name, and a graph that omits a file omits its definitions
/// too. An evidence ranking, not a probability.
pub const UNIQUE_NAME_CONFIDENCE: f32 = 0.6;

/// Confidence ceiling of a [`EdgeProvenance::Syntax`] edge. An extractor sees
/// one file, so half confidence is the most that view can support.
pub const MAX_SYNTAX_CONFIDENCE: f32 = 0.5;

/// Trust of a traversal step through one member of an ambiguous edge's candidate
/// set. An evidence ranking, not a probability.
pub const AMBIGUOUS_CANDIDATE_CONFIDENCE: f32 = 0.2;

/// Most ids an ambiguous edge keeps in `candidates`. Beyond this the list stays
/// empty and the edge is plain unresolved.
pub const MAX_CANDIDATES: usize = 8;

/// Confidence of a `Syntax` edge of `kind`: `0.3` for a call, `0.5` for an
/// import or reference. Every `SourceEdge::syntax` caller and
/// [`SourceEdge::unbind`] take their confidence from here.
pub fn syntax_confidence(kind: SourceEdgeKind) -> f32 {
    match kind {
        SourceEdgeKind::Calls => 0.3,
        _ => MAX_SYNTAX_CONFIDENCE,
    }
}

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
