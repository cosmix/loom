//! A feature's candidate follow-ups: the open entries of its findings ledger.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::plan_server::review::built::{parse_generated, run_review_paths};
use crate::planning::findings::{ledger_path, Finding, FindingStatus};
use crate::planning::project::Project;

/// The anchor of the review section that holds open questions, as `Source:` lines name it.
const QUESTIONS_ANCHOR: &str = "open-questions";
/// The file-name prefix of a run review's `Source:` line, before the plan slug.
const REVIEW_PREFIX: &str = "REVIEW-PLAN-";

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

/// The feature's open ledger entries in ledger order, each as a candidate. An entry imported from
/// a run review names that review and its stage; one written by hand names the ledger.
pub(super) fn ledger_candidates(project: &Project, feature: &str) -> Vec<CandidateView> {
    let Some(feature) = project.feature(feature) else {
        return Vec::new();
    };
    let ledger = ledger_path(&feature.name).to_string_lossy().into_owned();
    let open = feature
        .trace
        .findings
        .iter()
        .map(|traced| &traced.finding)
        .filter(|finding| finding.status == FindingStatus::Open);
    // The newest review path of each plan slug, looked up once however many entries name it.
    let mut reviews: HashMap<&str, Option<String>> = HashMap::new();
    open.map(|finding| candidate(project, finding, &ledger, &mut reviews))
        .collect()
}

fn candidate<'a>(
    project: &Project,
    finding: &'a Finding,
    ledger: &str,
    reviews: &mut HashMap<&'a str, Option<String>>,
) -> CandidateView {
    let source = finding.source.as_deref().and_then(ReviewSource::parse);
    let kind = match &source {
        Some(source) if source.anchor == QUESTIONS_ANCHOR => CandidateKind::Question,
        _ => CandidateKind::Finding,
    };
    let stage_id = source.as_ref().map_or("", |source| source.stage_id);
    let review = source
        .as_ref()
        .and_then(|source| source.slug)
        .and_then(|slug| {
            reviews
                .entry(slug)
                .or_insert_with(|| newest_review(project, slug))
                .clone()
        })
        .unwrap_or_else(|| ledger.to_owned());
    CandidateView {
        id: finding.id.clone(),
        kind,
        stage: if stage_id.is_empty() {
            "review"
        } else {
            stage_id
        }
        .to_owned(),
        stage_id: stage_id.to_owned(),
        text: finding.text.clone(),
        review,
    }
}

/// The path of the newest readable run review of `slug` by its Generated stamp, the last path
/// when no stamp parses.
fn newest_review(project: &Project, slug: &str) -> Option<String> {
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
        .map(|(_, path)| path)
        .or_else(|| reviews.last().map(|(path, _)| path))?;
    Some(newest.to_string_lossy().into_owned())
}

/// A `Source:` value the review import wrote:
/// `REVIEW-PLAN-<slug>.md#<section anchor> (<stage id>, <hash>)`.
struct ReviewSource<'a> {
    /// The plan slug in the review's file name, when it has the expected shape.
    slug: Option<&'a str>,
    /// The review section's anchor.
    anchor: &'a str,
    /// The stage that recorded the item.
    stage_id: &'a str,
}

impl<'a> ReviewSource<'a> {
    fn parse(value: &'a str) -> Option<Self> {
        let (file, rest) = value.split_once('#')?;
        let (anchor, tail) = rest.split_once(" (")?;
        let (stage_id, _hash) = tail.strip_suffix(')')?.split_once(", ")?;
        let slug = file
            .strip_prefix(REVIEW_PREFIX)
            .and_then(|name| name.strip_suffix(".md"));
        Some(Self {
            slug,
            anchor,
            stage_id,
        })
    }
}

#[cfg(test)]
#[path = "checklist_candidates_tests.rs"]
mod tests;
