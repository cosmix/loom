//! Building one feature's trace from the loaded project.
//!
//! The steps run in dependency order: the coverage claims of every entry, plan and stage line;
//! the ledger with its `About:` tokens resolved; the headings with their covers and findings;
//! the entries with what to revisit; the documents with their progress and settle state; then
//! the gaps and the frontier.

mod claims;
mod docs;
mod gaps;
mod ledger;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use super::Trace;
use crate::planning::checklist::{Step, StepMark};
use crate::planning::kinds::{plans_dir, section_path, DocKind};
use crate::planning::metaplan::MetaplanEntry;
use crate::planning::project::{Feature, Project};
use crate::planning::slug::PlanEntry;
use crate::planning::vision::VisionRow;
use claims::Claim;

/// The trace of `feature`, read from `project` alone.
pub fn build_trace(project: &Project, feature: &Feature) -> Trace {
    let cx = Cx::new(project, feature);
    let mut problems = Vec::new();
    let findings = ledger::findings(&cx, &mut problems);
    let mut headings = claims::headings(&cx);
    ledger::attach(&mut headings, &findings);
    let entries = ledger::entries(&cx, &headings, &findings);
    let documents = docs::documents(&cx, &mut problems);
    let gaps = gaps::gaps(&cx, &documents, &headings, &entries, &findings);
    let frontier = gaps::frontier(&gaps, &documents);
    Trace {
        documents,
        headings,
        entries,
        findings,
        gaps,
        frontier,
        problems,
    }
}

/// What every step reads: the project, the feature, its sections and plans in spine order and
/// the coverage claims.
struct Cx<'a> {
    project: &'a Project,
    feature: &'a Feature,
    sections: Vec<SectionSlot<'a>>,
    plans: Vec<PlanSlot<'a>>,
    claims: Vec<Claim>,
}

/// A product section: listed in the vision, written, or both.
struct SectionSlot<'a> {
    number: u32,
    path: PathBuf,
    /// Its row in the vision's sections table, the section's stub.
    row: Option<&'a VisionRow>,
    /// Whether the file exists.
    written: bool,
}

/// A plan of the feature: named by a metaplan entry, written in the feature home, or both.
struct PlanSlot<'a> {
    slug: &'a str,
    /// The first entry naming the plan, and its metaplan.
    entry: Option<(&'a Path, &'a MetaplanEntry)>,
    /// The winning plan file, when the plan is written.
    plan: Option<&'a PlanEntry>,
}

impl<'a> Cx<'a> {
    fn new(project: &'a Project, feature: &'a Feature) -> Self {
        let sections = section_slots(project, feature);
        let plans = plan_slots(project, feature);
        let claims = claims::claims(project, feature, &plans);
        Cx {
            project,
            feature,
            sections,
            plans,
            claims,
        }
    }

    fn name(&self) -> &'a str {
        &self.feature.name
    }

    /// The text of a readable Markdown document.
    fn text(&self, rel: &Path) -> Option<&'a str> {
        self.project.text(rel)
    }

    /// Whether `rel` is a document outside the locked set.
    fn unlocked(&self, rel: &Path) -> bool {
        let doc = self.project.docs.get(rel);
        doc.is_some_and(|doc| doc.lock.is_none())
    }

    /// Whether every plan the feature's metaplans name is done or retired: the checklist's Run
    /// step reads done (`Facts::run_mark`), whatever earlier step still has a gap.
    fn all_done(&self) -> bool {
        let steps = &self.feature.checklist.steps;
        steps.contains(&(Step::Run, StepMark::Done))
    }
}

/// The feature's metaplan path, `doc/plans/<feature>/index.md`.
fn metaplan_path(feature: &str) -> PathBuf {
    plans_dir(Some(feature)).join("index.md")
}

/// A count as the trace stores it.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// The vision's rows in table order, then the section files it does not list by number.
fn section_slots<'a>(project: &'a Project, feature: &'a Feature) -> Vec<SectionSlot<'a>> {
    let rows = feature.vision_rows.as_deref().unwrap_or_default();
    let mut slots: Vec<SectionSlot> = rows
        .iter()
        .map(|row| {
            let path = section_path(&feature.name, row.number);
            SectionSlot {
                number: row.number,
                written: project.docs.contains_key(&path),
                path,
                row: Some(row),
            }
        })
        .collect();
    let mut unlisted: Vec<SectionSlot> = project
        .tree
        .feature_docs(&feature.name)
        .filter_map(|doc| match doc.kind {
            DocKind::ProductSection { number, .. } => Some((number, &doc.path)),
            _ => None,
        })
        .filter(|(_, path)| !slots.iter().any(|slot| slot.path == **path))
        .map(|(number, path)| SectionSlot {
            number,
            path: path.clone(),
            row: None,
            written: true,
        })
        .collect();
    unlisted.sort_by_key(|slot| slot.number);
    slots.extend(unlisted);
    slots
}

/// The plans in spine order: each slug at its first metaplan entry (a written plan of this
/// feature, or no plan anywhere), then the feature's plans no entry names, by slug. An entry
/// naming another feature's plan has no slot here.
fn plan_slots<'a>(project: &'a Project, feature: &'a Feature) -> Vec<PlanSlot<'a>> {
    let mine = |plan: &PlanEntry| plan.feature.as_deref() == Some(feature.name.as_str());
    let mut slots: Vec<PlanSlot> = Vec::new();
    for (path, entries) in &feature.metaplans {
        for entry in entries {
            let Some(slug) = entry.slug.as_deref() else {
                continue;
            };
            let plan = project.tree.plan(slug);
            if slots.iter().any(|slot| slot.slug == slug) || plan.is_some_and(|p| !mine(p)) {
                continue;
            }
            let entry = Some((path.as_path(), entry));
            slots.push(PlanSlot { slug, entry, plan });
        }
    }
    for plan in project.tree.plans.values().filter(|&plan| mine(plan)) {
        if !slots.iter().any(|slot| slot.slug == plan.slug) {
            let (slug, entry) = (plan.slug.as_str(), None);
            slots.push(PlanSlot {
                slug,
                entry,
                plan: Some(plan),
            });
        }
    }
    slots
}
