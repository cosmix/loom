//! Problem details (code, path, line) of `crate::planning::problems`, through `Project::load`.

use std::path::Path;

use super::{tree, vision};
use crate::planning::kinds::VISION_PATH;
use crate::planning::project::{Problem, Project};

type Found<'a> = (&'static str, &'a Path, Option<usize>);

/// The `(code, path, line)` of every problem with one of `codes`, in the project's order.
fn found<'a>(project: &'a Project, codes: &[&str]) -> Vec<Found<'a>> {
    let wanted = project.problems.iter().filter(|p| codes.contains(&p.code));
    wanted.map(|p| (p.code, p.path.as_path(), p.line)).collect()
}

fn plan_with_dead_refs(slug: &str) -> String {
    let brief = format!("doc/plans/briefs/{slug}/beta/missing.md");
    format!("# Plan {slug}\n\nImplements: none\n[dead](gone.md)\n`{brief}`\n")
}

#[test]
fn implements_problems_carry_one_based_lines() {
    let temp = tree(&[
        (VISION_PATH, vision(&[1])),
        (
            "doc/plans/PLAN-a.md",
            "# A\n\nImplements: section_01, bogus\nImplements: none\n".to_string(),
        ),
        (
            "doc/plans/PLAN-b.md",
            "# B\n\nImplements: section_09\n".to_string(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let (a, b) = (
        Path::new("doc/plans/PLAN-a.md"),
        Path::new("doc/plans/PLAN-b.md"),
    );
    let got = found(&project, &["implements-broken", "implements-duplicate"]);
    let want = [
        ("implements-broken", a, Some(3)),
        ("implements-duplicate", a, Some(4)),
        ("implements-broken", b, Some(3)),
    ];
    assert_eq!(got, want);
}

#[test]
fn broken_references_carry_one_based_lines() {
    let temp = tree(&[
        (VISION_PATH, vision(&[1])),
        (
            "doc/product/section_01.md",
            "# One\n\nSee [gone](gone.md) and\n`doc/missing.md`.\n".to_string(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let section = Path::new("doc/product/section_01.md");
    let want = [
        ("broken-reference", section, Some(3)),
        ("broken-reference", section, Some(4)),
    ];
    assert_eq!(found(&project, &["broken-reference"]), want);
}

#[test]
fn locked_plans_report_no_dead_link_or_dead_brief() {
    let temp = tree(&[
        ("doc/plans/IN_PROGRESS-PLAN-x.md", plan_with_dead_refs("x")),
        ("doc/plans/archive/DONE-PLAN-y.md", plan_with_dead_refs("y")),
        ("doc/plans/PLAN-z.md", plan_with_dead_refs("z")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let z = Path::new("doc/plans/PLAN-z.md");
    let got = found(&project, &["broken-reference", "broken-brief-reference"]);
    let want = [
        ("broken-brief-reference", z, None),
        ("broken-reference", z, Some(4)),
        ("broken-reference", z, Some(5)),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_tree_without_doc_reports_doc_missing_on_doc() {
    let temp = tempfile::TempDir::new().unwrap();
    let project = Project::load(temp.path()).unwrap();
    let missing: Vec<&Problem> = project
        .problems
        .iter()
        .filter(|p| p.code == "doc-missing")
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].path, Path::new("doc"));
    assert_eq!(missing[0].line, None);
}
