//! Whether a citation still holds, and whether a summary still covers its whole source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{members_hash, SentenceState, SummaryEval, SummaryState};
use crate::planning::kinds::VISION_PATH;
use crate::planning::project::Project;
use crate::planning::summary::format::{FrontMatter, LinkTarget, SummaryFile};
use crate::planning::summary::store::Scope;

/// What a summary's citations are judged against.
enum Kind<'a> {
    /// A document summary of `source` (its current path).
    Document { source: &'a Path },
    /// An overview over `members`, citing raw passages of `sources` only.
    Overview {
        members: &'a [String],
        sources: Vec<PathBuf>,
        done: &'a BTreeMap<String, SummaryEval>,
    },
}

/// The judging context of one summary.
pub(super) struct Context<'a> {
    project: &'a Project,
    front: &'a FrontMatter,
    kind: Kind<'a>,
}

impl<'a> Context<'a> {
    /// The context of summary `id` holding `file`; `source` is set for a document summary.
    /// `done` holds the evaluations of the document summaries an overview may cite.
    pub(super) fn new(
        project: &'a Project,
        scope: &'a Scope,
        done: &'a BTreeMap<String, SummaryEval>,
        id: &str,
        file: &'a SummaryFile,
        source: Option<&'a Path>,
    ) -> Self {
        let kind = match (id, source) {
            ("product", _) => Kind::Overview {
                members: &scope.product,
                sources: vec![PathBuf::from(VISION_PATH)],
                done,
            },
            ("engineering", _) => Kind::Overview {
                members: &scope.engineering,
                sources: project
                    .metaplans
                    .iter()
                    .map(|(path, _)| path.clone())
                    .collect(),
                done,
            },
            (_, Some(source)) => Kind::Document { source },
            // `load_summary` never yields a file for a document without a source; judge it by nothing.
            (_, None) => Kind::Overview {
                members: &[],
                sources: Vec::new(),
                done,
            },
        };
        Context {
            project,
            front: &file.front,
            kind,
        }
    }

    /// Whether the citation `target` still holds.
    pub(super) fn holds(&self, target: &LinkTarget) -> bool {
        match (target, &self.kind) {
            (LinkTarget::Source { path, anchor }, Kind::Document { source }) => {
                let named = path == source || Path::new(&self.front.of) == path;
                let keys = [
                    cite_key(source, anchor),
                    cite_key(Path::new(&self.front.of), anchor),
                ];
                named && self.span_matches(source, anchor, &keys)
            }
            (LinkTarget::Source { path, anchor }, Kind::Overview { sources, .. }) => {
                sources.contains(path) && self.span_matches(path, anchor, &[cite_key(path, anchor)])
            }
            (LinkTarget::Sentence { summary, id }, Kind::Overview { members, done, .. }) => {
                members.contains(summary) && member_sentence_current(done, summary, id)
            }
            _ => false,
        }
    }

    /// Whether the span `anchor` of the readable document `doc` hashes as the first of `keys`
    /// that `cites` records. A missing `cites` entry or anchor does not hold.
    fn span_matches(&self, doc: &Path, anchor: &str, keys: &[String]) -> bool {
        if self.project.content_hash(doc).is_none() {
            return false;
        }
        let span = self
            .project
            .docs
            .get(doc)
            .and_then(|d| d.sections.find(anchor));
        let recorded = keys.iter().find_map(|key| self.front.cites.get(key));
        matches!((span, recorded), (Some(span), Some(hash)) if &span.hash == hash)
    }

    /// Whether the summary still covers everything: a document's `file` hash equals its
    /// source's, an overview's `members` map equals the members' hashes now.
    pub(super) fn complete(&self) -> bool {
        match &self.kind {
            Kind::Document { source } => self
                .front
                .file
                .as_deref()
                .is_some_and(|f| Some(f) == self.project.content_hash(source)),
            Kind::Overview { members, done, .. } => {
                let now: BTreeMap<String, String> = members
                    .iter()
                    .filter_map(|m| Some((m.clone(), members_hash(done.get(m)?.file.as_ref()?))))
                    .collect();
                now == self.front.members
            }
        }
    }
}

/// Whether member summary `summary` is present and holds sentence `id` in the current state.
fn member_sentence_current(done: &BTreeMap<String, SummaryEval>, summary: &str, id: &str) -> bool {
    done.get(summary).is_some_and(|eval| {
        eval.state != SummaryState::Missing
            && eval
                .sentences
                .iter()
                .any(|s| s.id == id && s.state == SentenceState::Current)
    })
}

/// The `cites` key of `anchor` in `path`: `path#anchor`, or `path` for the preamble.
pub fn cite_key(path: &Path, anchor: &str) -> String {
    let path = path.to_string_lossy();
    if anchor.is_empty() {
        path.into_owned()
    } else {
        format!("{path}#{anchor}")
    }
}
