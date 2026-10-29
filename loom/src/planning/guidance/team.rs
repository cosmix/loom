//! The missing documents and brief bundles a team drafting session may write.

use std::path::PathBuf;

use super::opening::plan_bundle;
use super::{AuthoringStart, StartError};
use crate::planning::checklist::Offer;
use crate::planning::kinds::{plan_path, section_path};
use crate::planning::project::Project;

/// The maximum number of in-process drafting teammates.
pub const MAX_TEAMMATES: usize = 5;

/// A feature's currently offered team drafting run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRun {
    /// The owning feature.
    pub feature: String,
    /// Whether this run drafts plans rather than sections.
    pub plans: bool,
    /// Missing documents, repository-relative, in reading order.
    pub documents: Vec<PathBuf>,
    /// Brief bundle directories of the missing plans; empty for sections.
    pub bundles: Vec<PathBuf>,
    /// The document count capped at `MAX_TEAMMATES`.
    pub teammates: usize,
}

/// Resolve a currently offered draft-all action and its individual document offers.
pub fn draft_run(project: &Project, offer_id: &str, feature: &str) -> Result<DraftRun, StartError> {
    if !matches!(offer_id, "draft-all-sections" | "draft-all-plans") {
        return Err(StartError::NotOffered(offer_id.into()));
    }
    let start = AuthoringStart::from_offer_id(offer_id, project, Some(feature))?;
    let plans = match start {
        AuthoringStart::Offer(Offer::DraftAllSections { .. }) => false,
        AuthoringStart::Offer(Offer::DraftAllPlans { .. }) => true,
        _ => return Err(StartError::NotOffered(offer_id.into())),
    };
    let feature_state = project
        .feature(feature)
        .ok_or_else(|| StartError::NotOffered(offer_id.into()))?;
    let offers = feature_state
        .checklist
        .primary
        .iter()
        .chain(&feature_state.checklist.secondary);
    let mut documents = Vec::new();
    let mut bundles = Vec::new();
    for offer in offers {
        match offer {
            Offer::WriteSection { number, .. } if !plans => {
                documents.push(section_path(feature, *number));
            }
            Offer::WritePlan { slug, .. } if plans => {
                documents.push(plan_path(Some(feature), slug));
                bundles.push(plan_bundle(feature, slug));
            }
            _ => {}
        }
    }
    Ok(DraftRun {
        feature: feature.into(),
        plans,
        teammates: documents.len().min(MAX_TEAMMATES),
        documents,
        bundles,
    })
}

#[cfg(test)]
pub(super) mod tests;
