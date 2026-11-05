//! What a feature's done plans left in their run reviews: the Built view.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::PlanRowView;
use crate::plan_server::review::built::{parse_generated, run_review_paths};
use crate::planning::project::Project;
use crate::planning::run_review::{review_items, ReviewItem, ReviewSection};
use crate::planning::slug::PlanState;

/// What the feature's done plans left in their run reviews.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltView {
    /// The newest run review of each done plan that has one, in plan-row order.
    pub reviews: Vec<RunReviewView>,
    /// The `## Spec Conformance` items of those reviews: what integration-verify changed in the
    /// spec or the build to make them agree.
    pub spec: Vec<SpecItemView>,
    /// Their open findings, then their open questions: the candidate follow-ups.
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

/// An open finding or question from a run review, offered as a follow-up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateView {
    /// Whether it was recorded as a finding or a question.
    pub kind: CandidateKind,
    /// The name of the stage that recorded it.
    pub stage: String,
    /// The id of the stage that recorded it.
    pub stage_id: String,
    /// The recorded text, on one line.
    pub text: String,
    /// Path of the run review it came from.
    pub review: String,
}

/// What the feature's done plans left in their run reviews.
pub(super) fn built_view(project: &Project, rows: &[PlanRowView]) -> BuiltView {
    let mut built = BuiltView::default();
    let mut questions = Vec::new();
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
        let spec = items(ReviewSection::SpecConformance).into_iter();
        built.spec.extend(spec.map(|item| SpecItemView {
            stage: item.stage_name,
            stage_id: item.stage_id,
            text: item.text,
            review: path.clone(),
        }));
        let candidate = |kind, item: ReviewItem| CandidateView {
            kind,
            stage: item.stage_name,
            stage_id: item.stage_id,
            text: item.text,
            review: path.clone(),
        };
        let findings = items(ReviewSection::OpenFindings).into_iter();
        built
            .candidates
            .extend(findings.map(|item| candidate(CandidateKind::Finding, item)));
        let asked = items(ReviewSection::OpenQuestions).into_iter();
        questions.extend(asked.map(|item| candidate(CandidateKind::Question, item)));
    }
    built.candidates.extend(questions);
    built
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
