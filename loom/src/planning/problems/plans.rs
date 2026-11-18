//! Metaplan entries, plan declarations and competing plan copies.

use std::path::Path;

use super::{features::listed_sections, problem, unlocked, Problem};
use crate::planning::implements::{Implements, ImplementsLine};
use crate::planning::kinds::{section_path, DocKind};
use crate::planning::metaplan::{entries, MetaplanEntry};
use crate::planning::project::Project;
use crate::planning::trace::tokens::stage_lines;

#[cfg(test)]
mod tests;

pub(super) fn collect(project: &Project, found: &mut Vec<Problem>) {
    metaplan_problems(project, found);
    implements_problems(project, found);
    anchor_problems(project, found);
    duplicate_implementations(project, found);
    copy_problems(project, found);
}

fn metaplan_problems(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        for (path, entries) in &feature.metaplans {
            metaplan_entries(project, path, entries, found);
        }
    }
    // Misplaced metaplans still receive entry diagnostics alongside needs-feature.
    let misplaced = project
        .tree
        .docs
        .iter()
        .filter(|doc| matches!(doc.kind, DocKind::Metaplan { feature: None }));
    for doc in misplaced {
        let Some(text) = project.text(&doc.path) else {
            continue;
        };
        metaplan_entries(project, &doc.path, &entries(text), found);
    }
}

fn metaplan_entries(
    project: &Project,
    path: &Path,
    entries: &[MetaplanEntry],
    found: &mut Vec<Problem>,
) {
    if !unlocked(project, path) {
        return;
    }
    for entry in entries.iter().filter(|entry| entry.plan_file.is_none()) {
        let message = format!("metaplan entry \"{}\" names no plan file", entry.heading);
        found.push(problem(
            "metaplan-entry-without-plan",
            path,
            Some(entry.line + 1),
            message,
        ));
    }
    foreign_plan_entries(project, path, entries, found);
}

fn foreign_plan_entries(
    project: &Project,
    path: &Path,
    entries: &[MetaplanEntry],
    found: &mut Vec<Problem>,
) {
    let Some(feature) = project.tree.get(path).and_then(|doc| doc.kind.feature()) else {
        return;
    };
    for entry in entries {
        let Some(slug) = &entry.slug else {
            continue;
        };
        let Some(plan) = project.tree.plan(slug) else {
            continue;
        };
        if plan.feature.as_deref() == Some(feature) {
            continue;
        }
        let actual = plan.path.display();
        let message = format!(
            "plan {slug} is listed here but lives at {actual}; move the plan into \
             doc/plans/{feature}/ or list it in its own metaplan"
        );
        found.push(problem(
            "metaplan-entry-foreign-plan",
            path,
            Some(entry.line + 1),
            message,
        ));
    }
}

fn implements_problems(project: &Project, found: &mut Vec<Problem>) {
    for (slug, parsed) in &project.implements {
        let Some(plan) = project.tree.plan(slug) else {
            continue;
        };
        let path = plan.path.as_path();
        if !unlocked(project, path) {
            continue;
        }
        if plan.feature.is_some() && parsed.value == Implements::Unmapped {
            let message = "plan has no Implements: line".to_string();
            found.push(problem("implements-missing", path, None, message));
        }
        declaration_problems(path, parsed, found);
        let Implements::Sections(numbers) = &parsed.value else {
            continue;
        };
        let listed = plan
            .feature
            .as_deref()
            .map(|feature| listed_sections(project, feature))
            .unwrap_or_default();
        let at = parsed.line.map(|line| line + 1);
        for number in numbers.iter().filter(|number| !listed.contains(*number)) {
            let message = if plan.feature.is_none() {
                format!("standalone plan names section_{number:02}; write Implements: none")
            } else {
                format!("Implements: section_{number:02} is not in the feature's vision table")
            };
            found.push(problem("implements-broken", path, at, message));
        }
    }
}

fn declaration_problems(path: &Path, parsed: &ImplementsLine, found: &mut Vec<Problem>) {
    for line in &parsed.duplicate_lines {
        let message = "plan has a second Implements: line".to_string();
        found.push(problem(
            "implements-duplicate",
            path,
            Some(line + 1),
            message,
        ));
    }
    let at = parsed.line.map(|line| line + 1);
    for token in &parsed.bad_tokens {
        let message = format!("Implements: token `{token}` does not name a product section");
        found.push(problem("implements-broken", path, at, message));
    }
}

/// Why `section_NN#anchor` of `feature` names nothing, or `None` when it resolves.
pub(super) fn unresolved_anchor(
    project: &Project,
    feature: &str,
    number: u32,
    anchor: &str,
) -> Option<String> {
    match project.docs.get(&section_path(feature, number)) {
        None => Some(format!("section_{number:02} does not exist")),
        Some(doc) if doc.sections.find(anchor).is_none() => {
            Some(format!("section_{number:02} has no heading `{anchor}`"))
        }
        Some(_) => None,
    }
}

/// Anchored `Implements:` tokens and `Implements (stage:<id>):` lines of unlocked feature plans.
fn anchor_problems(project: &Project, found: &mut Vec<Problem>) {
    for plan in project.tree.plans.values() {
        let (Some(feature), Some(doc)) = (plan.feature.as_deref(), project.docs.get(&plan.path))
        else {
            continue;
        };
        if doc.lock.is_some() {
            continue;
        }
        if let Some(parsed) = project.implements.get(&plan.slug) {
            let at = parsed.line.map(|line| line + 1);
            for (number, anchor) in &parsed.anchors {
                if let Some(reason) = unresolved_anchor(project, feature, *number, anchor) {
                    let message = format!("Implements: {reason}");
                    found.push(problem(
                        "implements-anchor-missing",
                        &plan.path,
                        at,
                        message,
                    ));
                }
            }
        }
        let text = project.text(&plan.path).unwrap_or_default();
        stage_line_problems(project, plan.path.as_path(), (feature, text), found);
    }
}

fn stage_line_problems(
    project: &Project,
    path: &Path,
    (feature, text): (&str, &str),
    found: &mut Vec<Problem>,
) {
    let stages = project.docs.get(path).map(|doc| &doc.sections.stages);
    for line in stage_lines(text) {
        let at = Some(line.line + 1);
        let id = &line.stage;
        if !stages.is_some_and(|stages| stages.iter().any(|(stage, _)| stage == id)) {
            let message = format!("Implements (stage:{id}): the plan has no stage `{id}`");
            found.push(problem("stage-line-unknown", path, at, message));
        }
        for token in &line.bad {
            let message =
                format!("Implements (stage:{id}): token `{token}` does not name a product section");
            found.push(problem("implements-broken", path, at, message));
        }
        for section in &line.refs {
            let Some(anchor) = &section.anchor else {
                continue;
            };
            if let Some(reason) = unresolved_anchor(project, feature, section.number, anchor) {
                let message = format!("Implements (stage:{id}): {reason}");
                found.push(problem("implements-anchor-missing", path, at, message));
            }
        }
    }
}

/// Plans of one feature that implement the same part of a section: the whole section, or the
/// same heading. Plans that each implement different headings of one section do not clash.
fn duplicate_implementations(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        for (number, slugs) in &feature.implemented_by {
            for slug in clashing(project, *number, slugs) {
                let Some(plan) = project.tree.plan(slug) else {
                    continue;
                };
                if !unlocked(project, &plan.path) {
                    continue;
                }
                let at = project.implements.get(slug).and_then(|parsed| parsed.line);
                let message = format!(
                    "section_{number:02} in feature {} is implemented by multiple plans: {}",
                    feature.name,
                    slugs.join(", ")
                );
                found.push(problem(
                    "implements-duplicate",
                    &plan.path,
                    at.map(|line| line + 1),
                    message,
                ));
            }
        }
    }
}

/// The slugs among `slugs` whose claim on section `number` overlaps another one's.
fn clashing<'a>(project: &Project, number: u32, slugs: &'a [String]) -> Vec<&'a String> {
    let parsed: Vec<Option<&ImplementsLine>> = slugs
        .iter()
        .map(|slug| project.implements.get(slug))
        .collect();
    let mut clash = Vec::new();
    for (index, slug) in slugs.iter().enumerate() {
        let Some(own) = parsed[index] else {
            continue;
        };
        let overlaps = |other: &ImplementsLine| {
            own.whole.contains(&number)
                || other.whole.contains(&number)
                || own
                    .anchors
                    .iter()
                    .any(|claim| claim.0 == number && other.anchors.contains(claim))
        };
        let others = parsed.iter().enumerate().filter(|(at, _)| *at != index);
        if others.filter_map(|(_, other)| *other).any(overlaps) {
            clash.push(slug);
        }
    }
    clash
}

fn copy_problems(project: &Project, found: &mut Vec<Problem>) {
    for plan in project.tree.plans.values() {
        let (slug, current) = (&plan.slug, plan.path.display());
        for stray in &plan.strays {
            let message = format!("stray copy of plan {slug}; the current file is {current}");
            found.push(problem("stray-plan-copy", stray, None, message));
        }
        for conflict in &plan.conflicts {
            let message = format!("duplicate plan slug {slug}; the winning file is {current}");
            found.push(problem("duplicate-plan-slug", conflict, None, message));
        }
    }
}
