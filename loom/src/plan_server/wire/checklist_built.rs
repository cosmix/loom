//! What a feature's done plans left in their run reviews: the Built view.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::PlanRowView;
use crate::fs::plan_review::SPEC_CONFORMANCE_PREFIX;
use crate::plan_server::review::built::{parse_generated, run_review_paths};
use crate::planning::project::Project;
use crate::planning::run_review::{review_items, ReviewItem, ReviewSection};
use crate::planning::slug::PlanState;

#[path = "checklist_built/candidates.rs"]
mod candidates;

/// What the feature's done plans left in their run reviews.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltView {
    /// The newest run review of each done plan that has one, in plan-row order.
    pub reviews: Vec<RunReviewView>,
    /// The `## Spec Conformance` items of those reviews: what integration-verify changed in the
    /// spec or the build to make them agree.
    pub spec: Vec<SpecItemView>,
    /// The open entries of the feature's findings ledger, in ledger order: the candidate
    /// follow-ups.
    pub candidates: Vec<CandidateView>,
}

/// One done plan's newest run review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunReviewView {
    /// The done plan's slug.
    pub slug: String,
    /// Repository-relative path of the review, including the `doc/` prefix.
    pub path: String,
}

/// One spec conformance outcome from a run review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecItemView {
    /// The name of the stage that recorded it.
    pub stage: String,
    /// The id of the stage that recorded it.
    pub stage_id: String,
    /// The recorded text, on one line.
    pub text: String,
    /// Path of the run review it came from.
    pub review: String,
}

/// What a candidate follow-up was recorded as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    /// An open finding.
    Finding,
    /// An open question.
    Question,
}

/// An open ledger entry, offered as a follow-up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateView {
    /// The ledger id of the finding, `F-<n>`.
    pub id: String,
    /// Whether it was recorded as a finding or a question.
    pub kind: CandidateKind,
    /// What recorded it: the stage id of an imported review item, `review` for a finding written
    /// by hand.
    pub stage: String,
    /// The id of the stage that recorded it, empty for a finding written by hand.
    pub stage_id: String,
    /// The recorded text, on one line.
    pub text: String,
    /// Path of the run review it was imported from, else of the ledger.
    pub review: String,
}

/// What the feature's done plans left in their run reviews, and the open entries of its ledger.
pub(super) fn built_view(project: &Project, feature: &str, rows: &[PlanRowView]) -> BuiltView {
    let mut built = BuiltView {
        candidates: candidates::ledger_candidates(project, feature),
        ..BuiltView::default()
    };
    for row in rows {
        let done = project
            .tree
            .plan(&row.slug)
            .is_some_and(|plan| plan.state == PlanState::Done);
        let Some((path, text)) = done.then(|| newest_review(project, &row.slug)).flatten() else {
            continue;
        };
        built.reviews.push(RunReviewView {
            slug: row.slug.clone(),
            path: path.clone(),
        });
        let items = |section| review_items(text, section);
        let asked = items(ReviewSection::OpenQuestions);
        let spec = unasked_spec_items(items(ReviewSection::SpecConformance), &asked);
        built.spec.extend(spec.map(|item| SpecItemView {
            stage: item.stage_name,
            stage_id: item.stage_id,
            text: item.text,
            review: path.clone(),
        }));
    }
    built
}

/// The spec conformance items that are not also a question: a question is a candidate, listed
/// from the ledger.
///
/// A spec-check question is written under both sections, but `## Spec Conformance` strips the
/// `spec-conformance:` prefix and `## Open Questions` keeps the raw text, so the question is
/// compared prefix-stripped.
fn unasked_spec_items<'a>(
    spec: Vec<ReviewItem>,
    asked: &'a [ReviewItem],
) -> impl Iterator<Item = ReviewItem> + 'a {
    let same = move |item: &ReviewItem, q: &ReviewItem| {
        let text = q
            .text
            .strip_prefix(SPEC_CONFORMANCE_PREFIX)
            .unwrap_or(&q.text);
        q.stage_id == item.stage_id && text.trim_start() == item.text
    };
    spec.into_iter()
        .filter(move |item| !asked.iter().any(|q| same(item, q)))
}

/// The newest readable run review of `slug` by its Generated stamp, the last path when no stamp
/// parses, with its text.
fn newest_review<'a>(project: &'a Project, slug: &str) -> Option<(String, &'a str)> {
    let reviews: Vec<(PathBuf, &str)> = run_review_paths(project, slug)
        .into_iter()
        .filter_map(|path| {
            let text = project.text(&path)?;
            Some((path, text))
        })
        .collect();
    let newest = reviews
        .iter()
        .filter_map(|(path, text)| Some((parse_generated(text)?, path)))
        .max_by_key(|(stamp, _)| *stamp)
        .map(|(_, path)| path.clone())
        .or_else(|| reviews.last().map(|(path, _)| path.clone()))?;
    let text = reviews.iter().find(|(path, _)| *path == newest)?.1;
    Some((newest.to_string_lossy().into_owned(), text))
}

#[cfg(test)]
#[path = "checklist_built_tests.rs"]
mod tests;
