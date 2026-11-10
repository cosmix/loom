//! The parsing steps of `derive`: each document's view, every plan's declarations, and each
//! feature's vision, metaplans, sections and checklist.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{file_name, Feature, ProjectDoc, Sources};
use crate::planning::checklist::{build_checklist, ParsedDocs};
use crate::planning::discover::{DocTree, Document};
use crate::planning::implements::{self, Implements, ImplementsLine};
use crate::planning::kinds::{plans_dir, product_dir, vision_path, DocKind};
use crate::planning::metaplan;
use crate::planning::paths::LockedSet;
use crate::planning::pressure;
use crate::planning::sections::sections_of;
use crate::planning::trace::settle::unsettled;
use crate::planning::trace::Trace;
use crate::planning::vision::{parse_sections_table, VisionProblem};
use crate::planning::xref::{references, Reference};

pub(super) type PlanPressure = BTreeMap<String, Vec<pressure::PressureRecord>>;

/// One feature's view; its trace is built once every feature is in place.
pub(super) fn feature(
    tree: &DocTree,
    sources: &Sources,
    locked: &LockedSet,
    pressure: &PlanPressure,
    implements: &BTreeMap<String, ImplementsLine>,
    name: String,
) -> Feature {
    let parsed = parse_docs(tree, sources, locked, pressure, &name);
    let checklist = build_checklist(tree, &parsed, &name);
    Feature {
        implemented_by: implemented_by(tree, implements, &name),
        name,
        vision_rows: parsed.vision_rows,
        metaplans: parsed.metaplans,
        checklist,
        trace: Trace::default(),
    }
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
pub(super) fn parse_plans(
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
pub(super) fn project_docs(
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
    let unsettled = match unsettled(text) {
        Cow::Borrowed(_) => None,
        Cow::Owned(text) => Some(sections_of(&text)),
    };
    ProjectDoc {
        doc: doc.clone(),
        title,
        open_questions: pressure::open_questions(text),
        sections,
        unsettled,
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
