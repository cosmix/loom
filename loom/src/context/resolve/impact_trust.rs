//! Reverse steps and the weakest-edge trust a reverse-impact path carries.

use super::ImpactHit;
use crate::context::source_graph::{
    EdgeProvenance, SourceEdge, SourceEdgeKind, AMBIGUOUS_CANDIDATE_CONFIDENCE,
};

/// One reverse step: an edge, walked through its target or through one member of
/// its candidate set.
#[derive(Debug, Clone, Copy)]
pub(super) struct Step<'a> {
    pub(super) edge: &'a SourceEdge,
    pub(super) candidate: bool,
}

impl Step<'_> {
    /// A candidate step trusts [`AMBIGUOUS_CANDIDATE_CONFIDENCE`], whatever the
    /// edge's own confidence says.
    pub(super) fn confidence(&self) -> f32 {
        if self.candidate {
            AMBIGUOUS_CANDIDATE_CONFIDENCE
        } else {
            self.edge.confidence
        }
    }
}

/// The weakest edge seen so far along one path.
#[derive(Debug, Clone, Copy)]
pub(super) struct Trust {
    pub(super) min_confidence: f32,
    pub(super) provenance: EdgeProvenance,
    pub(super) kind: SourceEdgeKind,
    pub(super) via_candidates: bool,
}

impl Trust {
    /// Extend a path by `step`. `None` is the start node, which has no path yet.
    /// The candidate flag is sticky: it survives even when a later, weaker step
    /// becomes the weakest.
    pub(super) fn extend(current: Option<Trust>, step: Step<'_>) -> Trust {
        let confidence = step.confidence();
        let via_candidates = step.candidate || current.is_some_and(|t| t.via_candidates);
        match current {
            Some(trust) if trust.min_confidence <= confidence => Trust {
                via_candidates,
                ..trust
            },
            _ => Trust {
                min_confidence: confidence,
                provenance: step.edge.provenance,
                kind: step.edge.kind,
                via_candidates,
            },
        }
    }
}

impl From<&ImpactHit> for Trust {
    fn from(hit: &ImpactHit) -> Self {
        Trust {
            min_confidence: hit.min_confidence,
            provenance: hit.weakest_provenance,
            kind: hit.weakest_kind,
            via_candidates: hit.via_candidates,
        }
    }
}
