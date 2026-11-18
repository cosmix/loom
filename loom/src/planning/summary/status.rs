//! Machine-readable and human-readable summary status reports.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::claims::{list_claims, Claim};
use super::evaluate::{evaluate, SentenceState, SummaryEval, SummaryState};
use super::store::{expected_ids, summary_path};
use crate::context::untrusted::inline_safe;
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;

/// A versioned report of expected summaries and orphaned store files.
#[derive(Serialize)]
pub struct StatusReport {
    schema: u32,
    summaries: Vec<StatusSummary>,
}

#[derive(Serialize)]
struct StatusSummary {
    id: String,
    state: &'static str,
    source: Option<PathBuf>,
    path: PathBuf,
    sentences: Vec<StatusSentence>,
    claim: Option<StatusClaim>,
    error: Option<String>,
}

#[derive(Serialize)]
struct StatusSentence {
    id: String,
    line: usize,
    state: &'static str,
    text: String,
}

#[derive(Serialize)]
struct StatusClaim {
    session: Option<String>,
    pid: u32,
    created_at: String,
}

impl From<&Claim> for StatusClaim {
    fn from(claim: &Claim) -> Self {
        Self {
            session: claim.session.clone(),
            pid: claim.pid,
            created_at: claim.created_at.clone(),
        }
    }
}

impl From<&SummaryEval> for StatusSummary {
    fn from(summary: &SummaryEval) -> Self {
        Self {
            id: summary.id.clone(),
            state: summary.state.as_str(),
            source: summary.source.clone(),
            path: summary_path(&summary.id),
            sentences: summary
                .sentences
                .iter()
                .map(|sentence| StatusSentence {
                    id: sentence.id.clone(),
                    line: sentence.line,
                    state: sentence.state.as_str(),
                    text: sentence.text.clone(),
                })
                .collect(),
            claim: summary.claim.as_ref().map(StatusClaim::from),
            error: summary.error.clone(),
        }
    }
}

/// Load the project and claims, then report summaries in project order followed by orphans.
pub fn status_report(root: &Path, paths: &RuntimePaths) -> anyhow::Result<StatusReport> {
    let project = Project::load(root)?;
    let claims = list_claims(paths)?;
    let evaluation = evaluate(&project, &claims);
    let mut summaries: Vec<StatusSummary> = expected_ids(&project)
        .iter()
        .filter_map(|id| evaluation.summaries.get(id).map(StatusSummary::from))
        .collect();
    for id in &evaluation.orphans {
        summaries.push(StatusSummary {
            id: id.clone(),
            state: SummaryState::Missing.as_str(),
            source: None,
            path: summary_path(id),
            sentences: Vec::new(),
            claim: claims
                .iter()
                .find(|claim| claim.summary == *id)
                .map(StatusClaim::from),
            error: Some("orphan summary has no source in the project".to_string()),
        });
    }
    Ok(StatusReport {
        schema: 1,
        summaries,
    })
}

/// Render one line per summary, including stale sentence counts and any claimant.
pub fn render_status(report: &StatusReport) -> String {
    let mut output = String::new();
    for summary in &report.summaries {
        let stale = summary
            .sentences
            .iter()
            .filter(|s| s.state == SentenceState::Stale.as_str())
            .count();
        output.push_str(&format!(
            "{} {}  {}/{} sentences",
            summary.state,
            summary.id,
            stale,
            summary.sentences.len()
        ));
        if let Some(claim) = &summary.claim {
            output.push_str(&format!(
                "  [claimed by {}]",
                inline_safe(claim.session.as_deref().unwrap_or("a terminal"))
            ));
        }
        output.push('\n');
    }
    output
}
