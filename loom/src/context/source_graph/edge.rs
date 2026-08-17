//! Edge types for the derived source graph: [`SourceEdge`], its
//! [`SourceEdgeKind`], and [`EdgeProvenance`].

use serde::{Deserialize, Serialize};

use super::{MAX_INFERRED_CONFIDENCE, MAX_RESOLVED_INFERRED_CONFIDENCE, UNRESOLVED_TARGET};

/// The relationship a [`SourceEdge`] encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceEdgeKind {
    /// Lexical containment: file contains symbol, type contains method.
    Contains,
    /// An import, `use`, or `require` of another module.
    Imports,
    /// A call expression.
    Calls,
    /// A non-call mention of an identifier.
    References,
    /// A trait/interface implementation.
    Implements,
    /// Subclassing or trait supertrait.
    Extends,
}

impl SourceEdgeKind {
    /// Stable lowercase name used in CLI output and fixture JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceEdgeKind::Contains => "contains",
            SourceEdgeKind::Imports => "imports",
            SourceEdgeKind::Calls => "calls",
            SourceEdgeKind::References => "references",
            SourceEdgeKind::Implements => "implements",
            SourceEdgeKind::Extends => "extends",
        }
    }
}

impl std::fmt::Display for SourceEdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an edge's claim comes from. The graph is never asserted to be
/// complete; this is how a consumer tells a fact from a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeProvenance {
    /// The grammar resolved both endpoints syntactically within one file.
    Parser,
    /// A language server resolved it. Reserved: nothing emits this today.
    Lsp,
    /// Heuristically matched across files, or a target that could not be
    /// resolved at all.
    ///
    /// An extractor must not exceed [`MAX_INFERRED_CONFIDENCE`]: it sees one
    /// file and cannot check a name against the rest of the graph.
    /// `crate::context::resolve` may raise a uniquely-matched edge as far as
    /// [`MAX_RESOLVED_INFERRED_CONFIDENCE`] — whole-graph uniqueness is
    /// evidence extraction never had — but never to `1.0`, and never to
    /// [`EdgeProvenance::Parser`].
    Inferred,
}

impl EdgeProvenance {
    /// Stable lowercase name used in CLI output and fixture JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeProvenance::Parser => "parser",
            EdgeProvenance::Lsp => "lsp",
            EdgeProvenance::Inferred => "inferred",
        }
    }
}

impl std::fmt::Display for EdgeProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One directed edge of the derived source graph.
///
/// Construct through [`SourceEdge::parser`] or [`SourceEdge::inferred`] so the
/// provenance/confidence invariant cannot be violated by accident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceEdge {
    /// [`crate::context::source_graph::SourceNode::id`] of the origin.
    pub from: String,
    /// [`crate::context::source_graph::SourceNode::id`] of the target, or
    /// [`UNRESOLVED_TARGET`].
    pub to: String,
    pub kind: SourceEdgeKind,
    pub provenance: EdgeProvenance,
    /// How much to trust this edge, in `0.0..=1.0`.
    pub confidence: f32,
    /// The identifier as written at the call/import site. Kept so an
    /// unresolved edge still names what it was looking for.
    #[serde(default)]
    pub symbol: String,
}

impl SourceEdge {
    /// An edge both of whose endpoints the grammar resolved within one file.
    pub fn parser(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: SourceEdgeKind,
        symbol: impl Into<String>,
    ) -> Self {
        SourceEdge {
            from: from.into(),
            to: to.into(),
            kind,
            provenance: EdgeProvenance::Parser,
            confidence: 1.0,
            symbol: symbol.into(),
        }
    }

    /// An edge whose target was guessed or could not be resolved.
    ///
    /// `confidence` is clamped to [`MAX_INFERRED_CONFIDENCE`]: an inferred edge
    /// can never present itself as authoritative.
    pub fn inferred(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: SourceEdgeKind,
        symbol: impl Into<String>,
        confidence: f32,
    ) -> Self {
        SourceEdge {
            from: from.into(),
            to: to.into(),
            kind,
            provenance: EdgeProvenance::Inferred,
            confidence: confidence.clamp(0.0, MAX_INFERRED_CONFIDENCE),
            symbol: symbol.into(),
        }
    }

    /// An edge naming a symbol whose definition was not found anywhere.
    pub fn unresolved(
        from: impl Into<String>,
        kind: SourceEdgeKind,
        symbol: impl Into<String>,
    ) -> Self {
        SourceEdge::inferred(from, UNRESOLVED_TARGET, kind, symbol, 0.2)
    }

    /// True when this edge does not name a resolved target.
    pub fn is_unresolved(&self) -> bool {
        self.to == UNRESOLVED_TARGET
    }

    /// Point an unresolved edge at a target that whole-graph resolution found,
    /// raising confidence to at most [`MAX_RESOLVED_INFERRED_CONFIDENCE`].
    ///
    /// Returns `false` and changes nothing when the edge is not eligible —
    /// which is the whole point of routing resolution through here rather than
    /// assigning the fields directly:
    ///
    /// - a [`EdgeProvenance::Parser`] edge is never touched. The grammar
    ///   already saw both endpoints in one file; a name match cannot improve
    ///   on that and must not overwrite it.
    /// - an already-resolved edge is never retargeted. Resolution refines a
    ///   gap, it does not relitigate a decision.
    ///
    /// The edge stays [`EdgeProvenance::Inferred`]. Nothing in this crate can
    /// promote an edge to `Parser`, because nothing outside the grammar has
    /// the evidence for it.
    pub fn resolve_to(&mut self, target: impl Into<String>, confidence: f32) -> bool {
        if self.provenance == EdgeProvenance::Parser || !self.is_unresolved() {
            return false;
        }
        self.to = target.into();
        self.confidence = confidence.clamp(0.0, MAX_RESOLVED_INFERRED_CONFIDENCE);
        true
    }
}
