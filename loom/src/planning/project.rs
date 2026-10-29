//! The project view: the doc/ tree, its features, standalone plans and problems.
//!
//! The view keeps the text of every Markdown document it read, so `refresh` re-reads only the
//! paths a watcher batch names and recomputes every derived field from the cached texts. A
//! document's references depend on the whole tree (paths, plan slugs, other documents'
//! anchors), so re-deriving everything is what keeps a refreshed view equal to a fresh `load`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::planning::checklist::{build_checklist, move_offers, Checklist, Offer, ParsedDocs};
use crate::planning::discover::{discover, read_doc, DocTree, Document};
use crate::planning::implements::{self, Implements, ImplementsLine};
use crate::planning::kinds::{plans_dir, product_dir, vision_path, DocKind};
use crate::planning::metaplan::{self, MetaplanEntry};
use crate::planning::paths::{LockReason, LockedSet};
use crate::planning::pressure;
use crate::planning::problems;
use crate::planning::sections::{normalized_hash, sections_of, DocSections, Span};
use crate::planning::vision::{parse_sections_table, VisionProblem, VisionRow};
use crate::planning::watch::Batch;
use crate::planning::xref::{references, Reference};

pub use crate::planning::problems::Problem;

/// The text of every Markdown document, or why it could not be read.
type Sources = BTreeMap<PathBuf, std::result::Result<String, String>>;
type PlanPressure = BTreeMap<String, Vec<pressure::PressureRecord>>;

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
    pub(crate) fn text(&self, rel: &Path) -> Option<&str> {
        self.sources.get(rel)?.as_deref().ok()
    }
}

/// Derive every field of the view from the tree and the document texts.
fn derive(root: PathBuf, tree: DocTree, sources: Sources) -> Result<Project> {
    let locked = LockedSet::scan(&root)?;
    let docs = project_docs(&tree, &sources, &locked);
    let (implements, pressure) = parse_plans(&tree, &sources);
    let features = tree
        .feature_names()
        .into_iter()
        .map(|name| {
            let parsed = parse_docs(&tree, &sources, &locked, &pressure, &name);
            let checklist = build_checklist(&tree, &parsed, &name);
            Feature {
                implemented_by: implemented_by(&tree, &implements, &name),
                name,
                vision_rows: parsed.vision_rows,
                metaplans: parsed.metaplans,
                checklist,
            }
        })
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
    };
    project.problems = problems::collect(&project);
    Ok(project)
}

/// Parse one feature's vision, metaplans and written sections with project-wide plan facts.
fn parse_docs(
    tree: &DocTree,
    sources: &Sources,
    locked: &LockedSet,
    pressure: &PlanPressure,
    feature: &str,
) -> ParsedDocs {
    let text = |rel: &Path| sources.get(rel).and_then(|source| source.as_deref().ok());
    let vision = text(&vision_path(feature));
    let vision_rows = vision.map_or(Err(VisionProblem::NoHeading), parse_sections_table);
    let metaplan_docs = tree
        .feature_docs(feature)
        .filter(|doc| matches!(&doc.kind, DocKind::Metaplan { .. }));
    let metaplans = metaplan_docs
        .map(|doc| {
            let entries = text(&doc.path).map(metaplan::entries);
            (doc.path.clone(), entries.unwrap_or_default())
        })
        .collect();
    ParsedDocs {
        vision_rows,
        metaplans,
        pressure: pressure.clone(),
        locked_files: feature_locked_files(locked, feature),
        sections_written: sections_written(tree, sources, feature),
    }
}

/// The existing locked files under the feature's product and plans directories.
fn feature_locked_files(locked: &LockedSet, feature: &str) -> Vec<PathBuf> {
    let (product, plans) = (product_dir(feature), plans_dir(Some(feature)));
    let mut files = locked.locked_paths();
    files.retain(|path| path.starts_with(&product) || path.starts_with(&plans));
    files
}

/// Parse every readable winning plan, including standalone plans.
fn parse_plans(
    tree: &DocTree,
    sources: &Sources,
) -> (BTreeMap<String, ImplementsLine>, PlanPressure) {
    let mut implements = BTreeMap::new();
    let mut pressure = BTreeMap::new();
    for plan in tree.plans.values() {
        let Some(Ok(text)) = sources.get(&plan.path) else {
            continue;
        };
        implements.insert(plan.slug.clone(), implements::parse(text));
        pressure.insert(plan.slug.clone(), pressure::records(text));
    }
    (implements, pressure)
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

/// Section numbers whose file holds a non-whitespace character, sorted. A file that exists
/// but cannot be read (invalid UTF-8, over the size bound) counts as written when it is not
/// empty: section 01 calls a section present when its file exists and is not empty.
fn sections_written(tree: &DocTree, sources: &Sources, feature: &str) -> Vec<u32> {
    let written = tree
        .feature_docs(feature)
        .filter_map(|doc| match &doc.kind {
            DocKind::ProductSection {
                feature: Some(_),
                number,
            } => {
                let present = match sources.get(&doc.path)? {
                    Ok(text) => text.contains(|c: char| !c.is_whitespace()),
                    Err(_) => doc.size > 0,
                };
                present.then_some(*number)
            }
            _ => None,
        });
    written.collect::<BTreeSet<u32>>().into_iter().collect()
}

/// Plans by the section numbers their `Implements:` line names, each plan once per section.
fn implemented_by(
    tree: &DocTree,
    implements: &BTreeMap<String, ImplementsLine>,
    feature: &str,
) -> BTreeMap<u32, Vec<String>> {
    let mut by_section: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let plans = tree
        .plans
        .values()
        .filter(|entry| entry.feature.as_deref() == Some(feature));
    for plan in plans {
        let Some(ImplementsLine {
            value: Implements::Sections(numbers),
            ..
        }) = implements.get(&plan.slug)
        else {
            continue;
        };
        for number in numbers {
            let slugs = by_section.entry(*number).or_default();
            if !slugs.contains(&plan.slug) {
                slugs.push(plan.slug.clone());
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
