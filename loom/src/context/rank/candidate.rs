//! The ranker's query and candidate types.
//!
//! Re-exported from [`crate::context::rank`], which is where every caller names
//! them.

use crate::context::schema::{
    Channel, ChunkId, Confidence, EdgeProvenance, SelectionReason, SourceEdgeKind,
};

/// What the caller is asking for.
#[derive(Debug, Clone, Default)]
pub struct RankQuery {
    /// Query text used for lexical and exact matching.
    pub text: String,
    /// Chunk ids the caller demands verbatim.
    pub required_ids: Vec<String>,
    /// Chunk ids referenced by stages this query depends on.
    pub stage_dependency_ids: Vec<String>,
    /// Project-relative paths the stages this query depends on changed: the
    /// stage signal and the worker-brief hook fill them from
    /// `StageQuery::dependency_paths` (the hook adds the paths its worker
    /// prompt names); the CLI leaves them empty.
    /// `rank_source` boosts nodes whose file is named here (A.23).
    pub dependency_paths: Vec<String>,
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
    /// Distinct query terms naming this candidate (for a chunk, `candidacy::named_terms`).
    /// Feeds the hook's emit floor through `ContextItem::matched_term_count`.
    pub matched_term_count: usize,
    /// Cap on the confidence the reasons alone would imply, when the rung
    /// ladder judged the evidence weaker than the reason names it. See
    /// [`RankedCandidate::confidence`] — read it there, never here: a consumer
    /// that calls `Confidence::from_reasons` directly silently ignores the cap.
    pub confidence_ceiling: Option<Confidence>,
    /// Why a graph neighbour was admitted; `None` for every other candidate.
    pub via: Option<NeighborVia>,
}

impl RankedCandidate {
    /// The confidence to publish for this candidate. This CAPS and never
    /// raises: it returns the WEAKER of what the reasons imply and
    /// [`RankedCandidate::confidence_ceiling`].
    ///
    /// A ceiling of `Some(Confidence::High)` therefore cannot promote a
    /// lexical-only candidate, and `None` behaves exactly as
    /// `Confidence::from_reasons` alone. Stated first because a "ceiling" that
    /// could also lift is the obvious footgun here, and nothing in the type
    /// prevents a future caller from setting one optimistically.
    ///
    /// This is the ONE place the two halves of the answer meet, so every
    /// consumer that renders or serializes a confidence must come through it.
    pub fn confidence(&self) -> Confidence {
        let from_reasons = Confidence::from_reasons(&self.reasons);
        match self.confidence_ceiling {
            Some(ceiling) => weaker(ceiling, from_reasons),
            None => from_reasons,
        }
    }
}

/// The weaker of two confidences.
///
/// Spelled out here rather than as `Ord` on [`Confidence`] in `schema.rs`
/// deliberately: a total order over a three-value trust label invites
/// comparisons that do not mean anything (`>`, sorting, ranges), and only this
/// one `min` is actually wanted anywhere in the codebase.
fn weaker(left: Confidence, right: Confidence) -> Confidence {
    if strength(left) <= strength(right) {
        left
    } else {
        right
    }
}

/// Order `Confidence`'s variants so [`weaker`] can compare two of them.
fn strength(confidence: Confidence) -> u8 {
    match confidence {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    }
}

/// The edge that admitted a graph neighbour, for the explanation rendered
/// beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct NeighborVia {
    /// Id of the node the neighbour was found from.
    pub seed: String,
    /// What the edge is.
    pub edge_kind: SourceEdgeKind,
    /// The edge's direction relative to the neighbour.
    pub direction: EdgeDirection,
    /// How the edge was established.
    pub provenance: EdgeProvenance,
    /// Line of the reference site, when the edge records one.
    pub site_line: Option<usize>,
}

/// Direction of a [`NeighborVia`] edge relative to the neighbour it explains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDirection {
    /// The neighbour calls or references the seed.
    Outgoing,
    /// The seed calls or references the neighbour.
    Incoming,
}

/// Why a graph neighbour is in the pack, e.g. ``called by `seed` at path:L12``.
///
/// The verb is read from the neighbour's side of the edge: `Outgoing` means the
/// neighbour is the subject (``calls `seed` ``), `Incoming` means the seed is
/// (``called by `seed` ``). Only the `Incoming` form names a site, because the
/// site lives in the seed's file, which the neighbour's own pointer does not
/// show. The seed is named by its scope and located by the path in its id
/// (`<path>#<kind>:<scope>`); an id of any other shape is shown whole.
///
/// Lives beside [`NeighborVia`] rather than in the packer because the ranker
/// prices it (`rank_source::expand::neighbour_tokens`) and the packer renders
/// it, and the ranker must not depend on the packer.
pub fn neighbor_explanation(via: &NeighborVia) -> String {
    let (verb, passive) = match via.edge_kind {
        SourceEdgeKind::Calls => ("calls", "called by"),
        SourceEdgeKind::Implements => ("implements", "implemented by"),
        SourceEdgeKind::Extends => ("extends", "extended by"),
        SourceEdgeKind::References | SourceEdgeKind::Contains | SourceEdgeKind::Imports => {
            ("references", "referenced by")
        }
    };
    let (path, name) = match via.seed.split_once('#') {
        Some((path, suffix)) => (path, suffix.split_once(':').map_or(suffix, |(_, n)| n)),
        None => ("", via.seed.as_str()),
    };
    match via.direction {
        EdgeDirection::Outgoing => format!("{verb} `{name}`"),
        EdgeDirection::Incoming => match via.site_line {
            Some(line) if !path.is_empty() => format!("{passive} `{name}` at {path}:L{line}"),
            _ => format!("{passive} `{name}`"),
        },
    }
}
