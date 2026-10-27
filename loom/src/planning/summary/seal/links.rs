//! Link-target rules of a draft: what each citation may point at, the `cites` hashes it
//! records and the members it covers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::planning::kinds::VISION_PATH;
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{cite_key, Evaluation, SentenceState, SummaryState};
use crate::planning::summary::format::{Block, FormatError, LinkTarget, SummaryBody};
use crate::planning::summary::store;

/// The citations of a checked draft.
#[derive(Debug, Default)]
pub(super) struct Cited {
    /// `cites` front matter: `path#anchor` to the hash of the cited span.
    pub cites: BTreeMap<String, String>,
    /// Member summaries cited by at least one sentence link.
    members: BTreeSet<String>,
    /// Source documents cited by at least one passage link.
    sources: BTreeSet<PathBuf>,
}

/// What the draft of one summary is checked against.
pub(super) struct Rules<'a> {
    /// The loaded project.
    pub project: &'a Project,
    /// The summary being sealed.
    pub id: &'a str,
    /// Evaluation of the store, for overviews only.
    pub evaluation: Option<&'a Evaluation>,
}

impl Rules<'_> {
    fn is_overview(&self) -> bool {
        self.id == "product" || self.id == "engineering"
    }

    /// The documents a passage link may point at.
    pub(super) fn allowed_sources(&self) -> Vec<PathBuf> {
        match self.id {
            "product" => vec![PathBuf::from(VISION_PATH)],
            "engineering" => self
                .project
                .metaplans
                .iter()
                .map(|(p, _)| p.clone())
                .collect(),
            id => store::source_of(self.project, id).into_iter().collect(),
        }
    }

    /// The member summaries of an overview, in reading order.
    pub(super) fn members(&self) -> Vec<String> {
        let Some(evaluation) = self.evaluation else {
            return Vec::new();
        };
        match self.id {
            "product" => evaluation.scope.product.clone(),
            "engineering" => evaluation.scope.engineering.clone(),
            _ => Vec::new(),
        }
    }
}

/// Check every link of `body` and collect the problems, sorted by draft line.
pub(super) fn check(rules: &Rules<'_>, body: &SummaryBody) -> (Cited, Vec<FormatError>) {
    let allowed = rules.allowed_sources();
    let mut cited = Cited::default();
    let mut errors = Vec::new();
    let paragraphs = body.blocks.iter().filter_map(|block| match block {
        Block::Paragraph(sentences) => Some(sentences),
        Block::Heading { .. } => None,
    });
    for sentence in paragraphs.flatten() {
        let line = sentence.line + 1;
        for link in &sentence.links {
            let problem = match &link.resolved {
                LinkTarget::Unresolved(href) => Some(format!(
                    "link target {href} is neither a source document nor a summary sentence"
                )),
                LinkTarget::Source { path, anchor } => {
                    source_problem(rules, &allowed, path, anchor, &mut cited)
                }
                LinkTarget::Sentence { summary, id } => {
                    sentence_problem(rules, summary, id, &mut cited)
                }
            };
            errors.extend(problem.map(|message| FormatError { line, message }));
        }
    }
    errors.extend(uncited(rules, &allowed, &cited));
    errors.sort_by_key(|e| e.line);
    (cited, errors)
}

fn source_problem(
    rules: &Rules<'_>,
    allowed: &[PathBuf],
    path: &Path,
    anchor: &str,
    cited: &mut Cited,
) -> Option<String> {
    if !allowed.iter().any(|a| a == path) {
        return Some(format!(
            "link target {} is not a source of summary {}",
            path.display(),
            rules.id
        ));
    }
    let span = rules
        .project
        .docs
        .get(path)
        .and_then(|d| d.sections.find(anchor));
    let Some(span) = span.filter(|_| rules.project.content_hash(path).is_some()) else {
        return Some(format!("{} has no section #{anchor}", path.display()));
    };
    cited
        .cites
        .insert(cite_key(path, anchor), span.hash.clone());
    cited.sources.insert(path.to_path_buf());
    None
}

fn sentence_problem(
    rules: &Rules<'_>,
    summary: &str,
    id: &str,
    cited: &mut Cited,
) -> Option<String> {
    if !rules.is_overview() {
        return Some("a document summary cites only its source document".to_string());
    }
    if !rules.members().iter().any(|m| m == summary) {
        return Some(format!("{summary} is not a member of summary {}", rules.id));
    }
    let eval = rules.evaluation.and_then(|e| e.summaries.get(summary));
    let sentence = eval
        .filter(|e| e.state != SummaryState::Missing)
        .and_then(|e| e.sentences.iter().find(|s| s.id == id));
    match sentence {
        None => Some(format!("{summary} has no sentence {id}")),
        Some(s) if s.state != SentenceState::Current => {
            Some(format!("sentence {id} of {summary} is stale"))
        }
        Some(_) => {
            cited.members.insert(summary.to_string());
            None
        }
    }
}

/// Overviews cite every member and every source document at least once.
fn uncited(rules: &Rules<'_>, allowed: &[PathBuf], cited: &Cited) -> Vec<FormatError> {
    if !rules.is_overview() {
        return Vec::new();
    }
    let members = rules
        .members()
        .into_iter()
        .filter(|m| !cited.members.contains(m));
    let members = members.map(|m| format!("member summary {m} is not cited"));
    let sources = allowed.iter().filter(|p| !cited.sources.contains(*p));
    let sources = sources.map(|p| format!("source {} is not cited", p.display()));
    members
        .chain(sources)
        .map(|message| FormatError { line: 1, message })
        .collect()
}
