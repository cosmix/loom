//! Coverage claims: which entry, plan or stage names which section or heading, and the headings
//! they cover.

use std::path::{Path, PathBuf};

use super::{Cx, PlanSlot};
use crate::planning::implements::ImplementsLine;
use crate::planning::kinds::section_path;
use crate::planning::project::{Feature, Project};
use crate::planning::slug::PlanEntry;
use crate::planning::trace::tokens::{stage_lines, SectionRef};
use crate::planning::trace::{CoverKind, CoverRef, TraceHeading};

/// Section anchors that are not spec content and need no cover: `## Open questions` and the
/// section skill's `## Out of scope for this section` (plain `## Out of scope` too).
const UNCOVERABLE: [&str; 3] = [
    "open-questions",
    "out-of-scope",
    "out-of-scope-for-this-section",
];

/// One section or heading an edge line names.
pub(super) struct Claim {
    /// Who names it.
    pub(super) cover: CoverRef,
    /// The section's path.
    pub(super) path: PathBuf,
    /// The heading anchor; `None` claims the whole section.
    pub(super) anchor: Option<String>,
}

impl Claim {
    fn covers(&self, path: &Path, anchor: &str) -> bool {
        self.path == path && self.anchor.as_deref().is_none_or(|own| own == anchor)
    }
}

/// The claims of every metaplan entry's `Covers:` line, then of each written plan's
/// `Implements:` line and stage lines, in spine order.
pub(super) fn claims(project: &Project, feature: &Feature, plans: &[PlanSlot]) -> Vec<Claim> {
    let mut found = Vec::new();
    for (path, entries) in &feature.metaplans {
        for entry in entries {
            let cover = CoverRef {
                kind: CoverKind::Entry,
                path: path.clone(),
                anchor: Some(entry.anchor.clone()),
                slug: entry.slug.clone(),
                stage: None,
            };
            push(&mut found, &feature.name, &cover, &entry.covers);
        }
    }
    for plan in plans.iter().filter_map(|slot| slot.plan) {
        plan_claims(project, &feature.name, plan, &mut found);
    }
    found
}

fn plan_claims(project: &Project, feature: &str, plan: &PlanEntry, found: &mut Vec<Claim>) {
    let text = project.text(&plan.path).unwrap_or_default();
    let cover = |kind, stage: Option<&str>| CoverRef {
        kind,
        path: plan.path.clone(),
        anchor: stage.map(|id| format!("stage:{id}")),
        slug: Some(plan.slug.clone()),
        stage: stage.map(str::to_owned),
    };
    let declared = project
        .implements
        .get(&plan.slug)
        .map(ImplementsLine::claims);
    push(
        found,
        feature,
        &cover(CoverKind::Plan, None),
        &declared.unwrap_or_default(),
    );
    for line in stage_lines(text) {
        let stage = cover(CoverKind::Stage, Some(&line.stage));
        push(found, feature, &stage, &line.refs);
    }
}

/// One claim per token; a section named whole on the line drops its anchored tokens.
fn push(found: &mut Vec<Claim>, feature: &str, cover: &CoverRef, refs: &[SectionRef]) {
    let whole: Vec<u32> = refs
        .iter()
        .filter(|r| r.anchor.is_none())
        .map(|r| r.number)
        .collect();
    for section in refs {
        if section.anchor.is_some() && whole.contains(&section.number) {
            continue;
        }
        found.push(Claim {
            cover: cover.clone(),
            path: section_path(feature, section.number),
            anchor: section.anchor.clone(),
        });
    }
}

/// The level-2 headings of every written section in spine order, each with what covers it.
pub(super) fn headings(cx: &Cx) -> Vec<TraceHeading> {
    let mut out = Vec::new();
    for slot in cx.sections.iter().filter(|slot| slot.written) {
        let Some(doc) = cx.project.docs.get(&slot.path) else {
            continue;
        };
        let spec = doc.sections.headings.iter().filter(|heading| {
            heading.level == 2 && !UNCOVERABLE.contains(&heading.anchor.as_str())
        });
        for heading in spec {
            let mut covered_by: Vec<CoverRef> = Vec::new();
            let claims = cx
                .claims
                .iter()
                .filter(|c| c.covers(&slot.path, &heading.anchor));
            for claim in claims {
                if !covered_by.contains(&claim.cover) {
                    covered_by.push(claim.cover.clone());
                }
            }
            out.push(TraceHeading {
                path: slot.path.clone(),
                anchor: heading.anchor.clone(),
                title: heading.title.clone(),
                covered_by,
                findings: Vec::new(),
            });
        }
    }
    out
}
