//! Whether a citation still holds, and whether a summary still covers its whole source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{members_hash, SentenceState, SummaryEval, SummaryState};
use crate::planning::kinds::{classify, doc_id, vision_path};
use crate::planning::project::Project;
use crate::planning::summary::format::{FrontMatter, LinkTarget, SummaryFile};
use crate::planning::summary::store::Scope;

/// What a summary's citations are judged against.
enum Kind<'a> {
    /// A document summary of `source` (its current path).
    Document { source: &'a Path },
    /// An overview over `members`, citing raw passages of `sources` only.
    Overview {
        members: Vec<String>,
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
        done: &'a BTreeMap<String, SummaryEval>,
        id: &str,
        file: &'a SummaryFile,
        source: Option<&'a Path>,
    ) -> Self {
        let kind = if let Some(scope) = Scope::of(project, id) {
            let (members, sources) = if id.ends_with("/product") {
                (scope.product, vec![vision_path(&scope.feature)])
            } else {
                let sources = project
                    .feature(&scope.feature)
                    .into_iter()
                    .flat_map(|feature| feature.metaplans.iter().map(|(path, _)| path.clone()))
                    .collect();
                (scope.engineering, sources)
            };
            Kind::Overview {
                members,
                sources,
                done,
            }
        } else if let Some(source) = source {
            Kind::Document { source }
        } else {
            // A document without a source cannot have a loaded file.
            Kind::Overview {
                members: Vec::new(),
                sources: Vec::new(),
                done,
            }
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
                let named = self.names_source(path, source);
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

    /// Resolve a current path or stable document id, including a plan's old lifecycle path.
    fn names_source(&self, path: &Path, source: &Path) -> bool {
        if path == source || path == Path::new(&self.front.of) {
            return true;
        }
        let id = doc_id(path, &classify(path));
        self.project
            .tree
            .by_id(&id.0)
            .is_some_and(|doc| doc.path.as_path() == source)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::summary::evaluate::evaluate;
    use crate::planning::summary::evaluate::fixtures::*;

    #[test]
    fn document_citations_distinguish_the_same_section_number_in_two_features() {
        let f = two_sections();
        let other = "doc/product/other/section_01.md";
        put(&f.root, other, &section(1, "Alpha.", "Beta."));
        let mut file = seal_document(&f.root, ID1, &[("Alpha holds.", "a")]);
        let hash = file.front.cites.values().next().unwrap().clone();
        file.front
            .cites
            .insert(cite_key(Path::new(other), "a"), hash);
        let project = load(&f.root);
        let done = BTreeMap::new();
        let ctx = Context::new(&project, &done, ID1, &file, Some(Path::new(S1)));
        for (path, expected) in [
            (S1, true),
            ("product/demo/section_01", true),
            (other, false),
            ("product/other/section_01", false),
        ] {
            let citation = LinkTarget::Source {
                path: path.into(),
                anchor: "a".into(),
            };
            assert_eq!(ctx.holds(&citation), expected, "{path}");
        }
    }

    #[test]
    fn overview_citations_distinguish_two_features_with_identical_sources() {
        let f = two_sections();
        let other_vision = "doc/product/other/index.md";
        put(&f.root, other_vision, &vision(&[1, 2]));
        put(
            &f.root,
            "doc/product/other/section_01.md",
            &section(1, "Alpha.", "Beta."),
        );
        let other = "other/product/section_01";
        seal_document(&f.root, ID1, &[("Alpha holds.", "a")]);
        seal_document(&f.root, other, &[("Alpha holds.", "a")]);
        seal_overview(
            &f.root,
            "overview/other/product",
            &[(other, "Alpha holds."), (ID1, "Alpha holds.")],
            &[
                (other_vision, "specification-sections"),
                (VISION, "specification-sections"),
            ],
        );
        let evaluation = evaluate(&load(&f.root), &[]);
        let states: Vec<_> = evaluation.summaries["overview/other/product"]
            .sentences
            .iter()
            .map(|sentence| sentence.state)
            .collect();
        let (current, stale) = (SentenceState::Current, SentenceState::Stale);
        assert_eq!(states, vec![current, stale, current, stale]);
    }

    #[test]
    fn plan_citation_resolves_a_lifecycle_path_by_document_id() {
        let current = "doc/plans/demo/archive/DONE-PLAN-x.md";
        let f = fixture(&[
            (META, &metaplan(&["PLAN-x.md"])),
            (current, "# Plan: X\n\nDoes x.\n"),
        ]);
        let file = seal_document(&f.root, "plans/x", &[("X holds.", "plan-x")]);
        let project = load(&f.root);
        let done = BTreeMap::new();
        let ctx = Context::new(&project, &done, "plans/x", &file, Some(Path::new(current)));
        let citation = LinkTarget::Source {
            path: "doc/plans/demo/PLAN-x.md".into(),
            anchor: "plan-x".into(),
        };
        assert!(ctx.holds(&citation));
    }
}
