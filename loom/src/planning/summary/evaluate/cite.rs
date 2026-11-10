//! Whether a citation still holds, and whether a summary still covers its whole source.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{members_hash, SentenceState, SummaryEval, SummaryState};
use crate::planning::briefs::bundle_files;
use crate::planning::kinds::{classify, doc_id, vision_path};
use crate::planning::project::Project;
use crate::planning::summary::format::{FrontMatter, LinkTarget, SummaryFile};
use crate::planning::summary::hashes::SourceHashes;
use crate::planning::summary::store::Scope;

/// What a summary's citations are judged against.
enum Kind<'a> {
    /// A document summary of `source` (its current path). `bundle` holds the brief files of a
    /// plan's bundle, which a plan summary cites beside the plan; it is empty for every other
    /// document.
    Document {
        source: &'a Path,
        bundle: Vec<PathBuf>,
    },
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
    /// The source hashes citations and `file` are judged against.
    hashes: &'a SourceHashes<'a>,
    id: &'a str,
    front: &'a FrontMatter,
    kind: Kind<'a>,
    /// Ids of this summary's current sentences that cite no sentence of it: what a link to one
    /// of its own sentences may rest on.
    own: BTreeSet<String>,
}

/// The members and raw sources of overview `id`: a product overview's sections and its feature's
/// vision, an engineering overview's plans and its feature's metaplans. `None` for an id that
/// names no overview of a known feature.
pub(crate) fn overview_inputs(project: &Project, id: &str) -> Option<(Vec<String>, Vec<PathBuf>)> {
    let scope = Scope::of(project, id)?;
    if id.ends_with("/product") {
        return Some((scope.product, vec![vision_path(&scope.feature)]));
    }
    let sources = project
        .feature(&scope.feature)
        .into_iter()
        .flat_map(|feature| feature.metaplans.iter().map(|(path, _)| path.clone()))
        .collect();
    Some((scope.engineering, sources))
}

impl<'a> Context<'a> {
    /// The context of summary `id` holding `file`; `source` is set for a document summary.
    /// `done` holds the evaluations of the document summaries an overview may cite. No link to
    /// the summary's own sentences holds until [`Context::with_own`] names them.
    pub(super) fn new(
        hashes: &'a SourceHashes<'a>,
        done: &'a BTreeMap<String, SummaryEval>,
        id: &'a str,
        file: &'a SummaryFile,
        source: Option<&'a Path>,
    ) -> Self {
        let project = hashes.project();
        let kind = if let Some((members, sources)) = overview_inputs(project, id) {
            Kind::Overview {
                members,
                sources,
                done,
            }
        } else if let Some(source) = source {
            Kind::Document {
                source,
                bundle: bundle_files(project, source),
            }
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
            hashes,
            id,
            front: &file.front,
            kind,
            own: BTreeSet::new(),
        }
    }

    /// This context with `own` as the summary's current sentences that cite none of its own.
    pub(super) fn with_own(self, own: BTreeSet<String>) -> Self {
        Context { own, ..self }
    }

    /// Whether the citation `target` still holds.
    pub(super) fn holds(&self, target: &LinkTarget) -> bool {
        match (target, &self.kind) {
            (LinkTarget::Source { path, anchor }, Kind::Document { bundle, .. })
                if bundle.contains(path) =>
            {
                self.span_matches(path, anchor, &[cite_key(path, anchor)])
            }
            (LinkTarget::Source { path, anchor }, Kind::Document { source, .. }) => {
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
            (LinkTarget::Sentence { summary, id }, Kind::Document { .. }) if summary == self.id => {
                self.own.contains(id)
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
        let span = self.hashes.span(doc, anchor);
        let recorded = keys.iter().find_map(|key| self.front.cites.get(key));
        matches!((span, recorded), (Some(span), Some(hash)) if &span.hash == hash)
    }

    /// Whether the summary still covers everything: a document's `file` hash equals its
    /// source's and its `bundle` map equals its plan's bundle now (a brief added, removed or
    /// changed breaks it), an overview's `members` map equals the members' hashes now.
    pub(super) fn complete(&self) -> bool {
        match &self.kind {
            Kind::Document { source, bundle } => {
                let file = self.front.file.as_deref();
                file.is_some_and(|f| Some(f) == self.hashes.file_hash(source))
                    && bundle_hashes(self.project, bundle) == self.front.bundle
            }
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

/// The `bundle` front matter of a plan summary whose brief files are `files`: each brief's
/// repository path to its content hash. A brief the project holds no text for (not Markdown,
/// unreadable, oversized) is hashed by the fingerprint the planning trace also uses.
pub fn bundle_hashes(project: &Project, files: &[PathBuf]) -> BTreeMap<String, String> {
    files
        .iter()
        .filter_map(|path| {
            let hash = project
                .content_hash(path)
                .map(str::to_string)
                .or_else(|| project.fingerprint(path))?;
            Some((path.to_string_lossy().into_owned(), hash))
        })
        .collect()
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
        let (hashes, done) = (SourceHashes::new(&project), BTreeMap::new());
        let ctx = Context::new(&hashes, &done, ID1, &file, Some(Path::new(S1)));
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
        let (hashes, done) = (SourceHashes::new(&project), BTreeMap::new());
        let ctx = Context::new(&hashes, &done, "plans/x", &file, Some(Path::new(current)));
        let citation = LinkTarget::Source {
            path: "doc/plans/demo/PLAN-x.md".into(),
            anchor: "plan-x".into(),
        };
        assert!(ctx.holds(&citation));
    }
}
