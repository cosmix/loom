//! The project view: the doc/ tree, its features, standalone plans and problems.
//!
//! The view keeps the text of every Markdown document it read, so `refresh` re-reads only the
//! paths a watcher batch names and recomputes every derived field from the cached texts. A
//! document's references depend on the whole tree (paths, plan slugs, other documents'
//! anchors), so re-deriving everything is what keeps a refreshed view equal to a fresh `load`.

mod fingerprint;
mod parse;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use self::fingerprint::Fingerprints;

use crate::planning::checklist::{move_offers, Checklist, Offer};
use crate::planning::discover::{discover, read_doc, DocTree, Document};
use crate::planning::implements::ImplementsLine;
use crate::planning::metaplan::MetaplanEntry;
use crate::planning::paths::{LockReason, LockedSet};
use crate::planning::problems;
use crate::planning::sections::{normalized_hash, DocSections, Span};
use crate::planning::trace::{build_trace, Trace};
use crate::planning::vision::{VisionProblem, VisionRow};
use crate::planning::watch::Batch;
use crate::planning::xref::Reference;

pub use crate::planning::problems::Problem;

/// The text of every Markdown document, or why it could not be read.
type Sources = BTreeMap<PathBuf, std::result::Result<String, String>>;

/// One document of the loaded project with what was parsed from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDoc {
    /// The discovered document.
    pub doc: Document,
    /// The first level-1 heading's title, else the file name.
    pub title: String,
    /// Heading sections and hashes (empty for a non-Markdown or unreadable document).
    pub sections: DocSections,
    /// The sections of the text without its `Settled:` lines, which summaries hash; `None` when
    /// it holds none (`sections` stand).
    pub unsettled: Option<DocSections>,
    /// List items under `Open questions` headings.
    pub open_questions: usize,
    /// Cross-references the document makes.
    pub references: Vec<Reference>,
    /// Why the document is locked, if it is.
    pub lock: Option<LockReason>,
    /// Why the document could not be read, if it could not.
    pub read_error: Option<String>,
}

/// One feature's product and engineering documents and planning checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    /// The directory name shared by the feature's product and planning documents.
    pub name: String,
    /// The vision's section rows (`Err` also when there is no vision).
    pub vision_rows: Result<Vec<VisionRow>, VisionProblem>,
    /// This feature's metaplan entries, sorted by metaplan path.
    pub metaplans: Vec<(PathBuf, Vec<MetaplanEntry>)>,
    /// The slugs of this feature's plans implementing each section number.
    pub implemented_by: BTreeMap<u32, Vec<String>>,
    /// This feature's planning checklist.
    pub checklist: Checklist,
    /// This feature's planning trace (empty until `derive` builds it).
    pub trace: Trace,
}

/// The loaded project: what the planning server holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The repository root (the directory holding `doc/`).
    pub root: PathBuf,
    /// The discovered tree.
    pub tree: DocTree,
    /// Every document by repo-relative path.
    pub docs: BTreeMap<PathBuf, ProjectDoc>,
    /// The discovered features, sorted by name.
    pub features: Vec<Feature>,
    /// Sorted slugs of winning plans outside features.
    pub standalone: Vec<String>,
    /// Offers to move misplaced top-level documents into features.
    pub moves: Vec<Offer>,
    /// The `Implements:` declaration of every readable plan, by slug.
    pub implements: BTreeMap<String, ImplementsLine>,
    /// Convention problems, sorted by path, line and code.
    pub problems: Vec<Problem>,
    /// The locked documents.
    pub locked: LockedSet,
    /// The cached text of every Markdown document.
    sources: Sources,
    /// The content hash of every non-Markdown brief.
    fingerprints: Fingerprints,
}

impl Project {
    /// The feature with the given directory name.
    pub fn feature(&self, name: &str) -> Option<&Feature> {
        self.features.iter().find(|feature| feature.name == name)
    }

    /// Discover `doc/` under `root`, read every Markdown document and derive the view. A
    /// document that cannot be read gets a `read_error`; an error scanning the locked set
    /// fails the load (fail closed).
    pub fn load(root: &Path) -> Result<Project> {
        let tree = discover(root)?;
        let markdown = tree.docs.iter().filter(|doc| is_markdown(&doc.path));
        let sources = markdown
            .map(|doc| (doc.path.clone(), read_source(root, &doc.path)))
            .collect();
        let fingerprints = fingerprint::collect(root, &tree, Fingerprints::new(), &BTreeSet::new());
        derive(root.to_path_buf(), tree, (sources, fingerprints))
    }

    /// Apply a watcher batch: re-read every path it names that is still a document (and any
    /// document the cache lacks), drop the documents that went away, and re-derive the rest.
    /// The result equals a fresh `load` of the same tree.
    pub fn refresh(&mut self, batch: &Batch) -> Result<()> {
        let tree = discover(&self.root)?;
        let renamed = batch.renamed.iter().flat_map(|(from, to)| [from, to]);
        let named = batch
            .added
            .iter()
            .chain(&batch.changed)
            .chain(&batch.removed);
        let stale: BTreeSet<&Path> = named.chain(renamed).map(PathBuf::as_path).collect();
        // Taken, not cloned: if deriving fails the cache stays empty and the next refresh
        // re-reads every document.
        let mut sources = std::mem::take(&mut self.sources);
        sources.retain(|path, _| tree.get(path).is_some() && !stale.contains(path.as_path()));
        for doc in tree.docs.iter().filter(|doc| is_markdown(&doc.path)) {
            let root = &self.root;
            let path = &doc.path;
            sources
                .entry(path.clone())
                .or_insert_with(|| read_source(root, path));
        }
        let cached = std::mem::take(&mut self.fingerprints);
        let fingerprints = fingerprint::collect(&self.root, &tree, cached, &stale);
        *self = derive(self.root.clone(), tree, (sources, fingerprints))?;
        Ok(())
    }

    /// The `file_hash` of a readable Markdown document.
    pub fn content_hash(&self, rel: &Path) -> Option<&str> {
        let doc = self.docs.get(rel)?;
        let readable = is_markdown(rel) && doc.read_error.is_none();
        readable.then_some(doc.sections.file_hash.as_str())
    }

    /// The span `anchor` addresses in `rel` and its text, re-read from disk. `Ok(None)` when
    /// the document or anchor is unknown, or the file changed since the view read it (the
    /// caller refreshes first).
    pub fn section_text(&self, rel: &Path, anchor: &str) -> Result<Option<(Span, String)>> {
        let Some(cached) = self.content_hash(rel) else {
            return Ok(None);
        };
        let text = read_doc(&self.root, rel)?;
        if normalized_hash(&text) != cached {
            return Ok(None);
        }
        let span = self.docs.get(rel).and_then(|doc| doc.sections.find(anchor));
        let Some(span) = span else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let span_lines = lines
            .get(span.start_line..span.end_line)
            .unwrap_or_default();
        let mut body = span_lines.join("\n");
        if span.end_line > span.start_line {
            body.push('\n');
        }
        Ok(Some((span.clone(), body)))
    }

    /// What stands for the content of a document `text` returns nothing for: the content hash
    /// of a non-Markdown brief, or the size of a file the project could not read (all it holds
    /// of it). `None` for a readable Markdown document and for an unknown path.
    pub(crate) fn fingerprint(&self, rel: &Path) -> Option<String> {
        if let Some(print) = self.fingerprints.get(rel) {
            return Some(print.clone());
        }
        let doc = self.docs.get(rel)?;
        doc.read_error
            .as_ref()
            .map(|_| format!("unread: {} bytes", doc.doc.size))
    }

    /// The cached text of a readable Markdown document.
    pub(crate) fn text(&self, rel: &Path) -> Option<&str> {
        self.sources.get(rel)?.as_deref().ok()
    }
}

/// Derive every field of the view from the tree and the document texts.
fn derive(
    root: PathBuf,
    tree: DocTree,
    (sources, fingerprints): (Sources, Fingerprints),
) -> Result<Project> {
    let locked = LockedSet::scan(&root)?;
    let docs = parse::project_docs(&tree, &sources, &locked);
    let (implements, pressure) = parse::parse_plans(&tree, &sources);
    let features = tree
        .feature_names()
        .into_iter()
        .map(|name| parse::feature(&tree, &sources, &locked, &pressure, &implements, name))
        .collect();
    let standalone = tree
        .plans
        .values()
        .filter(|plan| plan.feature.is_none())
        .map(|plan| plan.slug.clone())
        .collect();
    let mut project = Project {
        root,
        moves: move_offers(&tree),
        tree,
        docs,
        features,
        standalone,
        implements,
        problems: Vec::new(),
        locked,
        sources,
        fingerprints,
    };
    // A trace reads the whole project (plan states, documents, other features' plans), so
    // every feature is in place before the first trace is built; the problems include theirs.
    let traces: Vec<Trace> = project
        .features
        .iter()
        .map(|feature| build_trace(&project, feature))
        .collect();
    for (feature, trace) in project.features.iter_mut().zip(traces) {
        feature.trace = trace;
    }
    project.problems = problems::collect(&project);
    Ok(project)
}

fn read_source(root: &Path, rel: &Path) -> std::result::Result<String, String> {
    read_doc(root, rel).map_err(|error| format!("{error:#}"))
}

fn is_markdown(rel: &Path) -> bool {
    rel.extension().is_some_and(|ext| ext == "md")
}

fn file_name(rel: &Path) -> String {
    rel.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}
