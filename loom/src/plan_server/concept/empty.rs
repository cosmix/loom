//! Empty overview states, scoped to one feature's documents.

use super::body::doc_link;
use crate::plan_server::wire::concept::{EmptyKind, EmptyState};
use crate::planning::checklist::Offer;
use crate::planning::kinds::{vision_path, DocKind};
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::summary_id_for;

/// Why an overview without a file cannot render.
pub(super) fn empty_state(project: &Project, eval: &Evaluation, feature: &str) -> EmptyState {
    let exists = project.tree.get(&vision_path(feature)).is_some();
    let documents: Vec<_> = project
        .docs
        .iter()
        .filter(|(_, doc)| {
            doc.doc.kind.feature() == Some(feature)
                && matches!(doc.doc.kind, DocKind::ProductSection { .. })
        })
        .collect();
    let mut empty = EmptyState {
        kind: EmptyKind::Building,
        count: None,
        offer: None,
        docs: Vec::new(),
    };
    if !exists && documents.is_empty() {
        empty.kind = EmptyKind::NoVision;
        empty.offer = Some(
            Offer::StartVision {
                feature: feature.into(),
            }
            .id(),
        );
    } else if !exists {
        empty.kind = EmptyKind::SectionsNoVision;
        empty.docs = documents
            .iter()
            .filter_map(|(path, _)| doc_link(project, path))
            .collect();
    } else if !documents.is_empty()
        && documents.iter().all(|(path, _)| {
            summary_id_for(project, path)
                .and_then(|id| eval.summaries.get(&id))
                .is_some_and(|one| one.state == SummaryState::Missing)
        })
    {
        empty.kind = EmptyKind::NeverBuilt;
        empty.count = Some(documents.len() as u32);
    }
    empty
}
