//! Evaluation of every summary against the current project.
//!
//! Pure over the loaded [`Project`] plus the summary files, which are read no-follow
//! ([`read_summary`]). Document summaries are evaluated first, so an overview sentence can ask
//! whether the member sentence it cites is current.

#[cfg(test)]
mod bundle_tests;
mod cite;
#[cfg(test)]
mod feature_tests;
#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
mod levels_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vision_tests;

pub(crate) use cite::overview_inputs;
pub use cite::{bundle_hashes, cite_key};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::claims::Claim;
use super::format::{self, Block, FormatError, LinkTarget, Sentence, SummaryFile, SummaryKind};
use super::hashes::SourceHashes;
use super::store::{self, Scope};
use super::write::{read_summary, stored_ids};
use crate::planning::kinds::{classify, DocKind};
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;

/// State of one sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceState {
    /// Every citation still holds.
    Current,
    /// A cited span changed or vanished, or a cited summary sentence is gone or stale.
    Stale,
}

impl SentenceState {
    /// Lower-case name, as reports print it.
    pub fn as_str(self) -> &'static str {
        match self {
            SentenceState::Current => "current",
            SentenceState::Stale => "stale",
        }
    }
}

/// State of one summary, in precedence order from `Missing` down to `Current`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryState {
    /// Every sentence current and the source (or the member set) unchanged.
    Current,
    /// Every sentence current, but the source changed outside the cited spans, a brief of a
    /// plan's bundle changed, joined or left it, or a member summary changed, was added or was
    /// removed.
    Incomplete,
    /// At least one sentence stale.
    Stale,
    /// Claimed by a session; its sentences keep their states.
    Rebuilding,
    /// No file, or a file that fails validation or holds conflict markers.
    Missing,
}

impl SummaryState {
    /// Lower-case name, as reports print it.
    pub fn as_str(self) -> &'static str {
        match self {
            SummaryState::Current => "current",
            SummaryState::Incomplete => "incomplete",
            SummaryState::Stale => "stale",
            SummaryState::Rebuilding => "rebuilding",
            SummaryState::Missing => "missing",
        }
    }

    /// True for the states the server rebuilds without a press: stale and missing. An
    /// incomplete summary is out of date too, but says nothing its sources contradict: it only
    /// leaves out text no sentence cites (an edited "Open questions" list, a new paragraph), so
    /// it waits for a press, or for an edit of what it cites that stales it.
    pub fn rebuilds_in_background(self) -> bool {
        matches!(self, SummaryState::Stale | SummaryState::Missing)
    }
}

/// One citation link of a sentence and whether it still holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationEval {
    /// What the link points at.
    pub target: LinkTarget,
    /// Whether the cited span or sentence is unchanged and current.
    pub current: bool,
}

/// One evaluated sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceEval {
    /// Sentence id, `s-` and 8 hex digits.
    pub id: String,
    /// Sentence text without its links.
    pub text: String,
    /// 1-based line in the summary file.
    pub line: usize,
    /// Current when every citation holds.
    pub state: SentenceState,
    /// Every citation link, in order.
    pub citations: Vec<CitationEval>,
}

/// One evaluated summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryEval {
    /// Summary id.
    pub id: String,
    /// Current repository-relative source path (document summaries).
    pub source: Option<PathBuf>,
    /// The summary's state.
    pub state: SummaryState,
    /// The parsed file, unless missing.
    pub file: Option<SummaryFile>,
    /// Every sentence, in order (empty when missing).
    pub sentences: Vec<SentenceEval>,
    /// The claim naming this summary, if any.
    pub claim: Option<Claim>,
    /// Why the summary is missing.
    pub error: Option<String>,
}

/// Every expected summary evaluated, with feature overview scopes and the store's orphans.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// Evaluations by summary id.
    pub summaries: BTreeMap<String, SummaryEval>,
    /// Overview members and outside documents, keyed by feature name.
    pub scopes: BTreeMap<String, Scope>,
    /// Ids of store files no expected summary owns (the engine deletes them).
    pub orphans: Vec<String>,
}

/// Evaluate every expected summary of `project`; `claims` decide which are rebuilding.
pub fn evaluate(project: &Project, claims: &[Claim]) -> Evaluation {
    let scopes: BTreeMap<_, _> = project
        .features
        .iter()
        .map(|feature| (feature.name.clone(), store::scope(project, &feature.name)))
        .collect();
    let expected = store::expected_ids(project);
    let (overviews, documents): (Vec<&String>, Vec<&String>) =
        expected.iter().partition(|id| store::is_overview(id));
    let hashes = SourceHashes::new(project);
    let mut summaries = BTreeMap::new();
    for id in documents.into_iter().chain(overviews) {
        let claim = claims.iter().find(|c| &c.summary == id).cloned();
        let eval = evaluate_one(&hashes, &summaries, id, claim);
        summaries.insert(id.clone(), eval);
    }
    let orphans = orphans_among(project, &expected);
    Evaluation {
        summaries,
        scopes,
        orphans,
    }
}

/// Ids of store files that no expected summary of `project` owns.
pub(crate) fn orphans(project: &Project) -> Vec<String> {
    orphans_among(project, &store::expected_ids(project))
}

fn orphans_among(project: &Project, expected: &[String]) -> Vec<String> {
    match stored_ids(&project.root) {
        Ok(ids) => ids
            .into_iter()
            .filter(|id| !expected.contains(id))
            .collect(),
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "cannot list the summary store");
            Vec::new()
        }
    }
}

/// `normalized_hash` of the file's sentence ids in order, joined by `\n`: the value an
/// overview's `members` map records for a member summary.
pub fn members_hash(file: &SummaryFile) -> String {
    let ids: Vec<&str> = sentences_of(file).map(|s| s.id.as_str()).collect();
    normalized_hash(&ids.join("\n"))
}

/// Every sentence of a summary file, in order.
pub fn sentences_of(file: &SummaryFile) -> impl Iterator<Item = &Sentence> {
    file.blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph(sentences) => Some(sentences),
            Block::Heading { .. } => None,
        })
        .flatten()
}

fn evaluate_one(
    hashes: &SourceHashes<'_>,
    done: &BTreeMap<String, SummaryEval>,
    id: &str,
    claim: Option<Claim>,
) -> SummaryEval {
    let project = hashes.project();
    let source = store::source_of(project, id);
    let mut eval = SummaryEval {
        id: id.to_string(),
        source: source.clone(),
        state: SummaryState::Missing,
        file: None,
        sentences: Vec::new(),
        claim,
        error: None,
    };
    let file = match load_summary(project, id, source.as_deref()) {
        Ok(file) => file,
        Err(message) => {
            eval.error = Some(message);
            return eval;
        }
    };
    let ctx = cite::Context::new(hashes, done, id, &file, source.as_deref());
    let own = own_current(id, &file, &ctx);
    let ctx = ctx.with_own(own);
    eval.sentences = sentences_of(&file)
        .map(|s| sentence_eval(s, &ctx))
        .collect();
    eval.state = if eval.claim.is_some() {
        SummaryState::Rebuilding
    } else if eval
        .sentences
        .iter()
        .any(|s| s.state == SentenceState::Stale)
    {
        SummaryState::Stale
    } else if !ctx.complete() {
        SummaryState::Incomplete
    } else {
        SummaryState::Current
    };
    eval.file = Some(file);
    eval
}

/// Ids of the current sentences of summary `id` that cite none of its own sentences: a plan
/// summary's detail level, what its opening sentences rest on. Only those are evaluated here, so
/// opening sentences citing each other (a hand edit) read stale instead of recursing.
fn own_current(id: &str, file: &SummaryFile, ctx: &cite::Context<'_>) -> BTreeSet<String> {
    if !sentences_of(file).any(|s| format::cites_own(id, s)) {
        return BTreeSet::new();
    }
    sentences_of(file)
        .filter(|s| !format::cites_own(id, s))
        .map(|s| sentence_eval(s, ctx))
        .filter(|s| s.state == SentenceState::Current)
        .map(|s| s.id)
        .collect()
}

fn sentence_eval(sentence: &Sentence, ctx: &cite::Context<'_>) -> SentenceEval {
    let citations: Vec<CitationEval> = sentence
        .links
        .iter()
        .map(|link| CitationEval {
            target: link.resolved.clone(),
            current: ctx.holds(&link.resolved),
        })
        .collect();
    let current = !citations.is_empty() && citations.iter().all(|c| c.current);
    SentenceEval {
        id: sentence.id.clone(),
        text: sentence.text.clone(),
        line: sentence.line + 1,
        state: if current {
            SentenceState::Current
        } else {
            SentenceState::Stale
        },
        citations,
    }
}

/// Read, parse and check summary `id`; the error says why it counts as missing.
fn load_summary(project: &Project, id: &str, source: Option<&Path>) -> Result<SummaryFile, String> {
    let text = match read_summary(&project.root, id) {
        Ok(Some(text)) => text,
        Ok(None) => return Err("no summary file".to_string()),
        Err(error) => return Err(format!("{error:#}")),
    };
    let file = format::parse(&store::summary_path(id), &text).map_err(|e| describe(&e))?;
    let front = &file.front;
    let fits = match (front.kind == SummaryKind::Overview, source) {
        (true, _) => store::is_overview(id) && front.of == id,
        (false, Some(source)) => !store::is_overview(id) && same_source(id, &front.of, source),
        (false, None) => false,
    };
    if !fits {
        return Err(format!(
            "front matter names {:?} {}, not this summary's source",
            front.kind, front.of
        ));
    }
    Ok(file)
}

fn describe(errors: &[FormatError]) -> String {
    let lines: Vec<String> = errors
        .iter()
        .map(|e| format!("line {}: {}", e.line, e.message))
        .collect();
    lines.join("; ")
}

/// Whether `of` names summary `id`'s current `source`, or an earlier path of the same plan
/// (a rename the engine has not rewritten yet).
pub(crate) fn same_source(id: &str, of: &str, source: &Path) -> bool {
    if Path::new(of) == source {
        return true;
    }
    let Some(slug) = id.strip_prefix("plans/") else {
        return false;
    };
    of.starts_with("doc/")
        && matches!(classify(Path::new(of)), DocKind::Plan { slug: named, .. } if named == slug)
}
