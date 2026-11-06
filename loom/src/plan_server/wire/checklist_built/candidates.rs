//! The Built view's candidate follow-ups: the open entries of the feature's findings ledger.

use std::collections::HashMap;

use super::{newest_review, CandidateKind, CandidateView};
use crate::planning::findings::{ledger_path, Finding, FindingStatus};
use crate::planning::project::Project;

/// The anchor of the review section that holds open questions, as `Source:` lines name it.
const QUESTIONS_ANCHOR: &str = "open-questions";
/// The file-name prefix of a run review's `Source:` line, before the plan slug.
const REVIEW_PREFIX: &str = "REVIEW-PLAN-";

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
                .or_insert_with(|| newest_review(project, slug).map(|(path, _)| path))
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
