//! Link-target rules of a draft: what each citation may point at, the `cites` hashes it
//! records and the members it covers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::planning::briefs::bundle_files;
use crate::planning::project::Project;
use crate::planning::summary::evaluate::{
    cite_key, overview_inputs, Evaluation, SentenceEval, SentenceState, SummaryState,
};
use crate::planning::summary::format::{Block, FormatError, LinkTarget, Sentence, SummaryBody};
use crate::planning::summary::hashes::SourceHashes;
use crate::planning::summary::store;

use super::restate;

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
    /// The project's documents as summaries hash them (without their Settled lines).
    pub hashes: &'a SourceHashes<'a>,
    /// The summary being sealed.
    pub id: &'a str,
    /// Evaluation of the store, for overviews only.
    pub evaluation: Option<&'a Evaluation>,
}

impl Rules<'_> {
    /// The documents a passage link may point at: an overview's raw sources, or a document
    /// summary's source plus the brief bundle of a plan.
    pub(super) fn allowed_sources(&self) -> Vec<PathBuf> {
        if let Some((_, sources)) = overview_inputs(self.project, self.id) {
            return sources;
        }
        // The source first, then the brief files of a plan summary's bundle; every other
        // summary's source is no plan, so its bundle is empty.
        let source = store::source_of(self.project, self.id);
        let bundle = source
            .as_ref()
            .map(|plan| bundle_files(self.project, plan))
            .unwrap_or_default();
        source.into_iter().chain(bundle).collect()
    }

    /// The member summaries of an overview, in reading order.
    pub(super) fn members(&self) -> Vec<String> {
        overview_inputs(self.project, self.id)
            .map(|(members, _)| members)
            .unwrap_or_default()
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
        // A sentence citing one target twice reports each problem once.
        let mut reported = BTreeSet::new();
        // Member sentences this sentence cites without a problem: what opening it shows.
        let mut drawn = BTreeSet::new();
        for link in &sentence.links {
            let problem = match &link.resolved {
                LinkTarget::Unresolved(href) => Some(format!(
                    "link target {href} is neither a source document nor a summary sentence"
                )),
                LinkTarget::Source { path, anchor } => {
                    source_problem(rules, &allowed, path, anchor, &mut cited)
                }
                LinkTarget::Sentence { summary, id } => {
                    sentence_problem(rules, sentence, summary, id, &mut cited)
                }
            };
            if let (None, LinkTarget::Sentence { summary, id }) = (&problem, &link.resolved) {
                drawn.insert((summary.as_str(), id.as_str()));
            }
            let fresh = problem.filter(|message| reported.insert(message.clone()));
            errors.extend(fresh.map(|message| FormatError { line, message }));
        }
        let thin = single_member_problem(rules, sentence, &drawn);
        errors.extend(thin.map(|message| FormatError { line, message }));
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
    let Some(span) = rules.hashes.span(path, anchor) else {
        return Some(format!("{} has no section #{anchor}", path.display()));
    };
    cited
        .cites
        .insert(cite_key(path, anchor), span.hash.clone());
    cited.sources.insert(path.to_path_buf());
    None
}

/// Problem with the overview `sentence` citing sentence `id` of member `summary`: a bad target,
/// a member sentence that opens to detail sentences or is stale, or a sentence that only repeats
/// the one it cites. A plan summary's links to its own sentences are the levels' to check.
fn sentence_problem(
    rules: &Rules<'_>,
    sentence: &Sentence,
    summary: &str,
    id: &str,
    cited: &mut Cited,
) -> Option<String> {
    if !store::is_overview(rules.id) {
        if store::is_plan(rules.id) && summary == rules.id {
            return None;
        }
        return Some("a document summary cites only its source document".to_string());
    }
    if !rules.members().iter().any(|m| m == summary) {
        return Some(format!("{summary} is not a member of summary {}", rules.id));
    }
    let eval = rules.evaluation.and_then(|e| e.summaries.get(summary));
    let member = eval
        .filter(|e| e.state != SummaryState::Missing)
        .and_then(|e| e.sentences.iter().find(|s| s.id == id));
    match member {
        None => Some(format!("{summary} has no sentence {id}")),
        Some(s) if !citable(s) => Some(format!(
            "sentence {id} of {summary} opens to detail sentences; cite those detail sentences \
             instead"
        )),
        Some(s) if s.state != SentenceState::Current => {
            Some(format!("sentence {id} of {summary} is stale"))
        }
        Some(s) => {
            cited.members.insert(summary.to_string());
            restate::restates(&sentence.text, &s.text).then(|| {
                format!(
                    "sentence {} restates {summary}#{id}; say what the cited sentences add up \
                     to instead of repeating one",
                    sentence.id
                )
            })
        }
    }
}

/// Whether an overview may cite member sentence `sentence`: it cites no summary sentence. A
/// plan summary's opening sentences cite its detail sentences, and only those are citable.
fn citable(sentence: &SentenceEval) -> bool {
    !sentence
        .citations
        .iter()
        .any(|citation| matches!(citation.target, LinkTarget::Sentence { .. }))
}

/// An overview sentence that opens to a single member sentence shows the reader little more
/// than itself, so it must cite at least two. The exemption is a member whose summary holds only
/// that one citable sentence: there is nothing else to cite.
fn single_member_problem(
    rules: &Rules<'_>,
    sentence: &Sentence,
    drawn: &BTreeSet<(&str, &str)>,
) -> Option<String> {
    if !store::is_overview(rules.id) {
        return None;
    }
    let (summary, _) = drawn.first().filter(|_| drawn.len() == 1)?;
    let only_sentence = rules
        .evaluation
        .and_then(|e| e.summaries.get(*summary))
        .is_some_and(|e| e.sentences.iter().filter(|s| citable(s)).count() == 1);
    (!only_sentence).then(|| {
        format!(
            "sentence {} cites one member sentence; an overview sentence draws on at least two",
            sentence.id
        )
    })
}

/// Overviews cite every member and every source document at least once; a plan summary cites
/// every brief of its bundle.
fn uncited(rules: &Rules<'_>, allowed: &[PathBuf], cited: &Cited) -> Vec<FormatError> {
    let messages: Vec<String> = if store::is_overview(rules.id) {
        let members = rules
            .members()
            .into_iter()
            .filter(|m| !cited.members.contains(m));
        let members = members.map(|m| format!("member summary {m} is not cited"));
        let sources = allowed.iter().filter(|p| !cited.sources.contains(*p));
        let sources = sources.map(|p| format!("source {} is not cited", p.display()));
        members.chain(sources).collect()
    } else {
        uncited_briefs(rules, allowed, cited)
    };
    messages
        .into_iter()
        .map(|message| FormatError { line: 1, message })
        .collect()
}

/// The briefs of a plan summary's bundle that no sentence cites. A brief the project holds no
/// text for (not Markdown, unreadable) has no anchor to cite, so it is not required; its hash
/// still goes into the `bundle` front matter.
fn uncited_briefs(rules: &Rules<'_>, allowed: &[PathBuf], cited: &Cited) -> Vec<String> {
    // `allowed` is the source followed by the bundle: drop the source, keep the briefs.
    let source = store::source_of(rules.project, rules.id);
    let briefs = allowed.iter().filter(|brief| {
        source.as_ref() != Some(*brief)
            && rules.project.content_hash(brief).is_some()
            && !cited.sources.contains(*brief)
    });
    briefs
        .map(|brief| format!("brief {} is not cited", brief.display()))
        .collect()
}
