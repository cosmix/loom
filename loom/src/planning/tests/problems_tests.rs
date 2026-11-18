//! Problem details (code, path, line) of `crate::planning::problems`, through `Project::load`.

use std::path::Path;

use super::{tree, vision};
use crate::planning::project::{Problem, Project};

mod implements;
mod metaplans;

const VISION: &str = "doc/product/a/index.md";

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

fn plan(sections: &str) -> String {
    format!("# Plan\nImplements: {sections}\n")
}

#[test]
fn implements_problems_carry_one_based_lines() {
    let temp = tree(&[
        (VISION, vision(&[1])),
        (
            "doc/plans/a/PLAN-a.md",
            "# A\n\nImplements: section_01, bogus\nImplements: none\n".to_string(),
        ),
        (
            "doc/plans/a/PLAN-b.md",
            "# B\n\nImplements: section_09\n".to_string(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let (a, b) = (
        Path::new("doc/plans/a/PLAN-a.md"),
        Path::new("doc/plans/a/PLAN-b.md"),
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
        (VISION, vision(&[1])),
        (
            "doc/product/a/section_01.md",
            "# One\n\nSee [gone](gone.md) and\n`doc/missing.md`.\n".to_string(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let section = Path::new("doc/product/a/section_01.md");
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

#[test]
fn top_level_feature_documents_need_a_feature_with_a_move_offer() {
    let temp = tree(&[
        ("doc/product/index.md", vision(&[5])),
        ("doc/product/section_05.md", "# Five\n".to_string()),
        ("doc/plans/index.md", "# Metaplan\n".to_string()),
        (VISION, vision(&[1])),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [
        ("needs-feature", Path::new("doc/plans/index.md"), None),
        ("needs-feature", Path::new("doc/product/index.md"), None),
        (
            "needs-feature",
            Path::new("doc/product/section_05.md"),
            None,
        ),
    ];
    assert_eq!(found(&project, &["needs-feature"]), want);
    for problem in project
        .problems
        .iter()
        .filter(|p| p.code == "needs-feature")
    {
        assert!(problem.message.contains("belongs in a feature directory"));
        let offer = "move it into doc/product/<feature>/ or doc/plans/<feature>/";
        assert!(problem.message.contains(offer));
    }
}

#[test]
fn locked_misplaced_documents_do_not_need_a_feature() {
    let temp = tree(&[("doc/product/index.md", "# Vision\n")]);
    let mut project = Project::load(temp.path()).unwrap();
    let path = Path::new("doc/product/index.md");
    project.docs.get_mut(path).unwrap().lock = Some(crate::planning::paths::LockReason::Archive);
    project.problems = crate::planning::problems::collect(&project);
    assert!(found(&project, &["needs-feature"]).is_empty());
}

#[test]
fn bad_feature_names_are_reported_once_per_name_at_the_first_directory() {
    let temp = tree(&[
        ("doc/plans/Bad/index.md", "# Metaplan\n"),
        ("doc/plans/Bad/notes.md", "# Notes\n"),
        ("doc/product/Bad/index.md", "# Vision\n"),
        ("doc/product/also_bad/section_01.md", "# One\n"),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [
        ("bad-feature-name", Path::new("doc/plans/Bad"), None),
        ("bad-feature-name", Path::new("doc/product/also_bad"), None),
    ];
    assert_eq!(found(&project, &["bad-feature-name"]), want);
    assert!(project
        .problems
        .iter()
        .filter(|p| p.code == "bad-feature-name")
        .all(|p| p
            .message
            .contains(p.path.file_name().unwrap().to_str().unwrap())));
}

#[test]
fn duplicate_plan_slugs_report_every_conflict_even_when_locked() {
    let winner = "doc/plans/a/archive/DONE-PLAN-same.md";
    let temp = tree(&[
        (winner, "# Winner\nImplements: none\n"),
        ("doc/plans/a/PLAN-same.md", "# Stray\nImplements: none\n"),
        ("doc/plans/b/PLAN-same.md", "# Conflict\nImplements: none\n"),
        (
            "doc/plans/c/archive/DONE-PLAN-same.md",
            "# Locked conflict\n",
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [
        (
            "duplicate-plan-slug",
            Path::new("doc/plans/b/PLAN-same.md"),
            None,
        ),
        (
            "duplicate-plan-slug",
            Path::new("doc/plans/c/archive/DONE-PLAN-same.md"),
            None,
        ),
    ];
    assert_eq!(found(&project, &["duplicate-plan-slug"]), want);
    assert!(project
        .problems
        .iter()
        .filter(|p| p.code == "duplicate-plan-slug")
        .all(|p| p.message.contains("same") && p.message.contains(winner)));
    let strays = [(
        "stray-plan-copy",
        Path::new("doc/plans/a/PLAN-same.md"),
        None,
    )];
    assert_eq!(found(&project, &["stray-plan-copy"]), strays);
}

#[test]
fn unlisted_sections_resolve_only_against_their_own_feature() {
    let temp = tree(&[
        (VISION, vision(&[5])),
        ("doc/product/b/index.md", vision(&[1])),
        ("doc/product/a/section_05.md", "# Five\n".to_string()),
        ("doc/product/b/section_05.md", "# Five\n".to_string()),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [(
        "unlisted-section",
        Path::new("doc/product/b/section_05.md"),
        None,
    )];
    assert_eq!(found(&project, &["unlisted-section"]), want);
}

#[test]
fn unreadable_visions_report_their_own_feature_paths_and_lines() {
    let temp = tree(&[
        (VISION, "# A\n".to_string()),
        (
            "doc/product/b/index.md",
            "# B\n\n## Specification sections\n".to_string(),
        ),
        ("doc/product/c/index.md", vision(&[1])),
        ("doc/plans/d/PLAN-engineering.md", plan("none")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [
        ("vision-table-unreadable", Path::new(VISION), None),
        (
            "vision-table-unreadable",
            Path::new("doc/product/b/index.md"),
            Some(3),
        ),
    ];
    assert_eq!(found(&project, &["vision-table-unreadable"]), want);
}

#[test]
fn feature_document_kinds_require_titles() {
    let temp = tree(&[
        (VISION, "Vision without a title\n"),
        ("doc/product/a/section_01.md", "Section without a title\n"),
        ("doc/plans/a/index.md", "Metaplan without a title\n"),
        ("doc/plans/a/PLAN-plan.md", "Implements: none\n"),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let code = "missing-title";
    let want = [
        (code, Path::new("doc/plans/a/PLAN-plan.md"), Some(1)),
        (code, Path::new("doc/plans/a/index.md"), Some(1)),
        (code, Path::new(VISION), Some(1)),
        (code, Path::new("doc/product/a/section_01.md"), Some(1)),
    ];
    assert_eq!(found(&project, &[code]), want);
}
