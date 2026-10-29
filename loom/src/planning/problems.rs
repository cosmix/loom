//! Convention problems found in the doc/ tree, as `loom plan check` reports them (section 09).
//!
//! A locked document (a running, done or retired plan, its briefs, a run review, the archive)
//! cannot be edited, so it reports no document problem; plan copies and invalid feature
//! directories are reported wherever they are.
//!
//! Problem codes: `doc-missing`, `needs-feature`, `bad-feature-name`, `missing-title`,
//! `vision-table-unreadable`, `unlisted-section`, `metaplan-entry-without-plan`,
//! `metaplan-entry-foreign-plan`, `implements-missing`, `implements-broken`,
//! `implements-duplicate`, `stray-plan-copy`, `duplicate-plan-slug`, `broken-reference`,
//! `broken-brief-reference`.

use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

use crate::planning::briefs::plan_briefs;
use crate::planning::discover::DocDirState;
use crate::planning::kinds::DocKind;
use crate::planning::project::Project;
use crate::planning::sections::DocSections;

mod features;
mod plans;

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
    features::collect(project, &mut found);
    title_problems(project, &mut found);
    plans::collect(project, &mut found);
    reference_problems(project, &mut found);
    brief_problems(project, &mut found);
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

/// A vision, product section, metaplan or plan whose first non-blank line is not a level-1
/// heading.
fn title_problems(project: &Project, found: &mut Vec<Problem>) {
    for (path, doc) in &project.docs {
        let titled_kind = matches!(
            doc.doc.kind,
            DocKind::Vision { .. }
                | DocKind::ProductSection { .. }
                | DocKind::Metaplan { .. }
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
