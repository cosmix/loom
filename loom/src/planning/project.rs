//! The project view: the doc/ tree, its parsed documents, problems and checklist.
//!
//! The view keeps the text of every Markdown document it read, so `refresh` re-reads only the
//! paths a watcher batch names and recomputes every derived field from the cached texts. A
//! document's references depend on the whole tree (paths, plan slugs, other documents'
//! anchors), so re-deriving everything is what keeps a refreshed view equal to a fresh `load`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::planning::checklist::{build_checklist, Checklist, ParsedDocs};
use crate::planning::discover::{discover, read_doc, DocTree, Document};
use crate::planning::implements::{self, Implements, ImplementsLine};
use crate::planning::kinds::{DocKind, VISION_PATH};
use crate::planning::metaplan::{self, MetaplanEntry};
use crate::planning::paths::{LockReason, LockedSet};
use crate::planning::pressure;
use crate::planning::problems::{self, Problem};
use crate::planning::sections::{normalized_hash, sections_of, DocSections, Span};
use crate::planning::vision::{parse_sections_table, VisionProblem, VisionRow};
use crate::planning::watch::Batch;
use crate::planning::xref::{references, Reference};

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
    /// List items under `Open questions` headings.
    pub open_questions: usize,
    /// Cross-references the document makes.
    pub references: Vec<Reference>,
    /// Why the document is locked, if it is.
    pub lock: Option<LockReason>,
    /// Why the document could not be read, if it could not.
    pub read_error: Option<String>,
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
    /// The vision's section rows (`Err` also when there is no vision).
    pub vision_rows: Result<Vec<VisionRow>, VisionProblem>,
    /// Every metaplan's entries, sorted by metaplan path.
    pub metaplans: Vec<(PathBuf, Vec<MetaplanEntry>)>,
    /// The `Implements:` declaration of every readable plan, by slug.
    pub implements: BTreeMap<String, ImplementsLine>,
    /// The slugs of the plans implementing each section number.
    pub implemented_by: BTreeMap<u32, Vec<String>>,
    /// The planning checklist.
    pub checklist: Checklist,
    /// Convention problems, sorted by path, line and code.
    pub problems: Vec<Problem>,
    /// The locked documents.
    pub locked: LockedSet,
    /// The cached text of every Markdown document.
    sources: Sources,
}

impl Project {
    /// Discover `doc/` under `root`, read every Markdown document and derive the view. A
    /// document that cannot be read gets a `read_error`; an error scanning the locked set
    /// fails the load (fail closed).
    pub fn load(root: &Path) -> Result<Project> {
        let tree = discover(root)?;
        let markdown = tree.docs.iter().filter(|doc| is_markdown(&doc.path));
        let sources = markdown
            .map(|doc| (doc.path.clone(), read_source(root, &doc.path)))
            .collect();
        derive(root.to_path_buf(), tree, sources)
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
        *self = derive(self.root.clone(), tree, sources)?;
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

    /// The cached text of a readable Markdown document.
    pub(super) fn text(&self, rel: &Path) -> Option<&str> {
        self.sources.get(rel)?.as_deref().ok()
    }
}

/// Derive every field of the view from the tree and the document texts.
fn derive(root: PathBuf, tree: DocTree, sources: Sources) -> Result<Project> {
    let locked = LockedSet::scan(&root)?;
    let docs = project_docs(&tree, &sources, &locked);
    let (parsed, implements) = parse_docs(&tree, &sources, &locked);
    let checklist = build_checklist(&tree, &parsed);
    let mut project = Project {
        root,
        implemented_by: implemented_by(&implements),
        tree,
        docs,
        vision_rows: parsed.vision_rows,
        metaplans: parsed.metaplans,
        implements,
        checklist,
        problems: Vec::new(),
        locked,
        sources,
    };
    project.problems = problems::collect(&project);
    Ok(project)
}

/// Parse the vision table, metaplans, `Implements` lines and pressure records of every
/// readable document.
fn parse_docs(
    tree: &DocTree,
    sources: &Sources,
    locked: &LockedSet,
) -> (ParsedDocs, BTreeMap<String, ImplementsLine>) {
    let text = |rel: &Path| sources.get(rel).and_then(|source| source.as_deref().ok());
    let vision = text(Path::new(VISION_PATH));
    let vision_rows = vision.map_or(Err(VisionProblem::NoHeading), parse_sections_table);
    let metaplan_docs = tree
        .docs
        .iter()
        .filter(|doc| metaplan::is_metaplan(&doc.path));
    let metaplans = metaplan_docs
        .map(|doc| {
            let entries = text(&doc.path).map(metaplan::entries);
            (doc.path.clone(), entries.unwrap_or_default())
        })
        .collect();
    let plan_texts: Vec<(&String, &str)> = tree
        .plans
        .values()
        .filter_map(|plan| Some((&plan.slug, text(&plan.path)?)))
        .collect();
    let implements: BTreeMap<String, ImplementsLine> = plan_texts
        .iter()
        .map(|&(slug, text)| (slug.clone(), implements::parse(text)))
        .collect();
    let pressure = plan_texts
        .iter()
        .map(|&(slug, text)| (slug.clone(), pressure::records(text)))
        .collect();
    let parsed = ParsedDocs {
        vision_rows,
        metaplans,
        pressure,
        locked_files: locked.locked_paths(),
        sections_written: sections_written(tree, sources),
    };
    (parsed, implements)
}

/// Every document's view, then the references of each readable Markdown document, which
/// resolve anchors against the other documents' sections.
fn project_docs(
    tree: &DocTree,
    sources: &Sources,
    locked: &LockedSet,
) -> BTreeMap<PathBuf, ProjectDoc> {
    let mut docs: BTreeMap<PathBuf, ProjectDoc> = tree
        .docs
        .iter()
        .map(|doc| {
            (
                doc.path.clone(),
                project_doc(doc, sources.get(&doc.path), locked),
            )
        })
        .collect();
    let lookup = |rel: &Path| {
        let doc = docs.get(rel).filter(|doc| doc.read_error.is_none());
        doc.map(|doc| doc.sections.clone())
    };
    let found: Vec<(PathBuf, Vec<Reference>)> = sources
        .iter()
        .filter_map(|(path, source)| {
            let text = source.as_deref().ok()?;
            Some((path.clone(), references(path, text, tree, &lookup)))
        })
        .collect();
    for (path, refs) in found {
        if let Some(doc) = docs.get_mut(&path) {
            doc.references = refs;
        }
    }
    docs
}

fn project_doc(
    doc: &Document,
    source: Option<&std::result::Result<String, String>>,
    locked: &LockedSet,
) -> ProjectDoc {
    let (text, read_error) = match source {
        Some(Ok(text)) => (text.as_str(), None),
        Some(Err(error)) => ("", Some(error.clone())),
        None => ("", None),
    };
    let sections = sections_of(text);
    let h1 = sections.headings.iter().find(|heading| heading.level == 1);
    let title = h1.map_or_else(|| file_name(&doc.path), |heading| heading.title.clone());
    ProjectDoc {
        doc: doc.clone(),
        title,
        open_questions: pressure::open_questions(text),
        sections,
        references: Vec::new(),
        lock: locked.lock_reason(&doc.path),
        read_error,
    }
}

/// Section numbers whose file holds a non-whitespace character, sorted.
fn sections_written(tree: &DocTree, sources: &Sources) -> Vec<u32> {
    let written = tree.docs.iter().filter_map(|doc| match doc.kind {
        DocKind::ProductSection { number } => {
            let text = sources.get(&doc.path)?.as_deref().ok()?;
            text.contains(|c: char| !c.is_whitespace())
                .then_some(number)
        }
        _ => None,
    });
    written.collect::<BTreeSet<u32>>().into_iter().collect()
}

/// Plans by the section numbers their `Implements:` line names, each plan once per section.
fn implemented_by(implements: &BTreeMap<String, ImplementsLine>) -> BTreeMap<u32, Vec<String>> {
    let mut by_section: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for (slug, line) in implements {
        let Implements::Sections(numbers) = &line.value else {
            continue;
        };
        for number in numbers {
            let slugs = by_section.entry(*number).or_default();
            if !slugs.contains(slug) {
                slugs.push(slug.clone());
            }
        }
    }
    by_section
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
