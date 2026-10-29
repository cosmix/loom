//! Metaplan entries, plan declarations and competing plan copies.

use std::path::Path;

use super::{features::listed_sections, problem, unlocked, Problem};
use crate::planning::implements::{Implements, ImplementsLine};
use crate::planning::kinds::DocKind;
use crate::planning::metaplan::{entries, MetaplanEntry};
use crate::planning::project::Project;

pub(super) fn collect(project: &Project, found: &mut Vec<Problem>) {
    metaplan_problems(project, found);
    implements_problems(project, found);
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

fn duplicate_implementations(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        for (number, slugs) in &feature.implemented_by {
            if slugs.len() < 2 {
                continue;
            }
            for slug in slugs {
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
