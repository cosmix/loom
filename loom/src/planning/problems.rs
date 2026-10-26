//! Convention problems found in the doc/ tree, as `loom plan check` reports them (section 09).
//!
//! A locked document (a running, done or retired plan, its briefs, a run review, the archive)
//! cannot be edited, so it reports no problem; a stray plan copy is reported wherever it is.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

use crate::planning::briefs::plan_briefs;
use crate::planning::discover::DocDirState;
use crate::planning::implements::Implements;
use crate::planning::kinds::{DocKind, VISION_PATH};
use crate::planning::project::Project;
use crate::planning::sections::DocSections;
use crate::planning::vision::VisionProblem;

/// One convention problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// The problem code (`implements-missing`, ...).
    pub code: &'static str,
    /// The repo-relative file it is reported on (`doc` for `doc-missing`).
    pub path: PathBuf,
    /// The 1-based line, when the problem has one.
    pub line: Option<usize>,
    /// One sentence naming the thing.
    pub message: String,
}

/// Every problem of the project, sorted by path, line and code.
pub(crate) fn collect(project: &Project) -> Vec<Problem> {
    let mut found = Vec::new();
    if project.tree.doc_dir != DocDirState::Present {
        let message = "doc/ is missing or is a symlink".to_string();
        found.push(problem("doc-missing", Path::new("doc"), None, message));
    }
    vision_problems(project, &mut found);
    title_problems(project, &mut found);
    metaplan_problems(project, &mut found);
    implements_problems(project, &mut found);
    reference_problems(project, &mut found);
    brief_problems(project, &mut found);
    for plan in project.tree.plans.values() {
        for stray in &plan.strays {
            let (slug, current) = (&plan.slug, plan.path.display());
            let message = format!("stray copy of plan {slug}; the current file is {current}");
            found.push(problem("stray-plan-copy", stray, None, message));
        }
    }
    found.sort_by(|a, b| (&a.path, a.line, a.code).cmp(&(&b.path, b.line, b.code)));
    found
}

fn problem(code: &'static str, path: &Path, line: Option<usize>, message: String) -> Problem {
    Problem {
        code,
        path: path.to_path_buf(),
        line,
        message,
    }
}

/// Whether `rel` is a discovered document outside the locked set.
fn unlocked(project: &Project, rel: &Path) -> bool {
    project.docs.get(rel).is_some_and(|doc| doc.lock.is_none())
}

/// The section numbers the vision's table lists (none when it cannot be read).
fn listed_sections(project: &Project) -> BTreeSet<u32> {
    project
        .vision_rows
        .iter()
        .flatten()
        .map(|row| row.number)
        .collect()
}

fn vision_problems(project: &Project, found: &mut Vec<Problem>) {
    let vision = Path::new(VISION_PATH);
    if let Err(why) = &project.vision_rows {
        if unlocked(project, vision) {
            let line = match why {
                VisionProblem::NoHeading => None,
                VisionProblem::NoTable { heading_line }
                | VisionProblem::NoNumberedRows { heading_line } => Some(heading_line + 1),
            };
            let message = "the vision has no Specification sections table with a numbered row";
            found.push(problem(
                "vision-table-unreadable",
                vision,
                line,
                message.to_string(),
            ));
        }
    }
    let listed = listed_sections(project);
    for doc in &project.tree.docs {
        let DocKind::ProductSection { number } = doc.kind else {
            continue;
        };
        if !listed.contains(&number) && unlocked(project, &doc.path) {
            let message = format!("section {number:02} is not listed in the vision's table");
            found.push(problem("unlisted-section", &doc.path, None, message));
        }
    }
}

/// A vision, product section, metaplan or plan whose first non-blank line is not a level-1
/// heading.
fn title_problems(project: &Project, found: &mut Vec<Problem>) {
    for (path, doc) in &project.docs {
        let titled_kind = matches!(
            doc.doc.kind,
            DocKind::Vision
                | DocKind::ProductSection { .. }
                | DocKind::Metaplan
                | DocKind::Plan { .. }
        );
        let Some(text) = project.text(path) else {
            continue;
        };
        if titled_kind && doc.lock.is_none() && !starts_with_title(text, &doc.sections) {
            let message = "the document does not start with a level-1 title".to_string();
            found.push(problem("missing-title", path, Some(1), message));
        }
    }
}

fn starts_with_title(text: &str, sections: &DocSections) -> bool {
    let first = text.lines().position(|line| !line.trim().is_empty());
    let heading = sections.headings.first();
    matches!((first, heading), (Some(line), Some(h)) if h.level == 1 && h.span.start_line == line)
}

fn metaplan_problems(project: &Project, found: &mut Vec<Problem>) {
    for (path, entries) in &project.metaplans {
        if !unlocked(project, path) {
            continue;
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
    }
}

fn implements_problems(project: &Project, found: &mut Vec<Problem>) {
    let listed = listed_sections(project);
    for (slug, parsed) in &project.implements {
        let Some(plan) = project.tree.plan(slug) else {
            continue;
        };
        let path = plan.path.as_path();
        if !unlocked(project, path) {
            continue;
        }
        if parsed.value == Implements::Unmapped {
            let message = "plan has no Implements: line".to_string();
            found.push(problem("implements-missing", path, None, message));
        }
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
        let Implements::Sections(numbers) = &parsed.value else {
            continue;
        };
        for number in numbers.iter().filter(|number| !listed.contains(*number)) {
            let message = format!("Implements: section_{number:02} is not in the vision's table");
            found.push(problem("implements-broken", path, at, message));
        }
    }
}

fn reference_problems(project: &Project, found: &mut Vec<Problem>) {
    for (path, doc) in project.docs.iter().filter(|(_, doc)| doc.lock.is_none()) {
        for reference in doc.references.iter().filter(|r| r.target.is_none()) {
            let raw = &reference.raw;
            let message = format!("reference `{raw}` does not resolve to a document or heading");
            found.push(problem(
                "broken-reference",
                path,
                Some(reference.line + 1),
                message,
            ));
        }
    }
}

/// Inputs of every unlocked, readable plan that `loom run` could not follow.
fn brief_problems(project: &Project, found: &mut Vec<Problem>) {
    for plan in project.tree.plans.values() {
        let Some(doc) = project.docs.get(&plan.path) else {
            continue;
        };
        if doc.lock.is_some() || doc.read_error.is_some() {
            continue;
        }
        let stages: Vec<String> = doc
            .sections
            .stages
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let broken = match plan_briefs(&project.root, plan, &stages) {
            Ok(briefs) => briefs.broken,
            Err(error) => vec![format!("the plan's inputs cannot be listed: {error:#}")],
        };
        for reason in broken {
            let message = format!("brief reference does not resolve: {reason}");
            found.push(problem("broken-brief-reference", &plan.path, None, message));
        }
    }
}
