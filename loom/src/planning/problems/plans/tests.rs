//! Tests for the anchor and stage-line problems of `problems::plans`.

use std::fs;

use crate::planning::project::Project;

const VISION: &str =
    "# V\n\n## Specification sections\n\n| # | Name |\n| --- | --- |\n| 01 | S1 |\n";
const SECTION: &str = "# S1\n\n## One\n\nText.\n";

fn load(files: &[(&str, &str)]) -> Project {
    let temp = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        let path = temp.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    Project::load(temp.path()).unwrap()
}

fn found(project: &Project, code: &str) -> Vec<(String, Option<usize>)> {
    let wanted = project.problems.iter().filter(|p| p.code == code);
    wanted
        .map(|p| (p.path.display().to_string(), p.line))
        .collect()
}

fn plan_with(body: &str) -> Project {
    load(&[
        ("doc/product/a/index.md", VISION),
        ("doc/product/a/section_01.md", SECTION),
        ("doc/plans/a/PLAN-p.md", body),
    ])
}

#[test]
fn anchored_implements_that_resolve_report_nothing() {
    let project = plan_with("# P\n\nImplements: section_01#one\n");
    assert!(found(&project, "implements-anchor-missing").is_empty());
}

#[test]
fn missing_heading_or_section_is_reported_once_per_token() {
    let project = plan_with("# P\n\nImplements: section_01#one, section_01#gone, section_07#x\n");
    let plan = "doc/plans/a/PLAN-p.md".to_string();
    assert_eq!(
        found(&project, "implements-anchor-missing"),
        [(plan.clone(), Some(3)), (plan, Some(3))]
    );
    let messages: Vec<&str> = project
        .problems
        .iter()
        .filter(|p| p.code == "implements-anchor-missing")
        .map(|p| p.message.as_str())
        .collect();
    assert!(messages[0].contains("no heading `gone`"), "{messages:?}");
    assert!(
        messages[1].contains("section_07 does not exist"),
        "{messages:?}"
    );
}

#[test]
fn stage_lines_need_a_real_stage_and_resolving_anchors() {
    let body = "# P\n\nImplements: section_01\nImplements (stage:build): section_01#one, section_01#nope\n\
                Implements (stage:ghost): section_01#one, wat\n\n<!-- loom METADATA -->\n```yaml\n\
                stages:\n  - id: build\n    name: B\n```\n<!-- END loom METADATA -->\n";
    let project = plan_with(body);
    let plan = "doc/plans/a/PLAN-p.md".to_string();
    assert_eq!(
        found(&project, "stage-line-unknown"),
        [(plan.clone(), Some(5))]
    );
    assert_eq!(
        found(&project, "implements-anchor-missing"),
        [(plan.clone(), Some(4))]
    );
    assert_eq!(found(&project, "implements-broken"), [(plan, Some(5))]);
}

#[test]
fn locked_and_standalone_plans_report_no_anchor_problems() {
    let project = load(&[
        ("doc/product/a/index.md", VISION),
        ("doc/product/a/section_01.md", SECTION),
        (
            "doc/plans/a/DONE-PLAN-done.md",
            "# D\n\nImplements: section_01#gone\n",
        ),
        (
            "doc/plans/PLAN-solo.md",
            "# S\n\nImplements: section_01#gone\n",
        ),
    ]);
    assert!(found(&project, "implements-anchor-missing").is_empty());
    assert!(project.tree.plan("solo").is_some());
}

fn two_plans(first: &str, second: &str) -> Project {
    let section = "# S1\n\n## Alpha\n\nText.\n\n## Beta\n\nText.\n";
    let (first, second) = (
        format!("# P\n\nImplements: {first}\n"),
        format!("# Q\n\nImplements: {second}\n"),
    );
    load(&[
        ("doc/product/a/index.md", VISION),
        ("doc/product/a/section_01.md", section),
        ("doc/plans/a/PLAN-p.md", &first),
        ("doc/plans/a/PLAN-q.md", &second),
    ])
}

#[test]
fn plans_implementing_different_headings_of_a_section_do_not_clash() {
    let project = two_plans("section_01#alpha", "section_01#beta");
    assert!(found(&project, "implements-duplicate").is_empty());
}

#[test]
fn plans_implementing_the_same_heading_clash() {
    let project = two_plans("section_01#alpha", "section_01#alpha, section_01#beta");
    assert_eq!(found(&project, "implements-duplicate").len(), 2);
}

#[test]
fn a_whole_section_clashes_with_an_anchored_claim() {
    let project = two_plans("section_01", "section_01#beta");
    assert_eq!(found(&project, "implements-duplicate").len(), 2);
}

#[test]
fn only_the_clashing_plans_are_reported() {
    let project = load(&[
        ("doc/product/a/index.md", VISION),
        (
            "doc/product/a/section_01.md",
            "# S1\n\n## Alpha\n\nText.\n\n## Beta\n\nText.\n",
        ),
        (
            "doc/plans/a/PLAN-p.md",
            "# P\n\nImplements: section_01#alpha\n",
        ),
        (
            "doc/plans/a/PLAN-q.md",
            "# Q\n\nImplements: section_01#alpha\n",
        ),
        (
            "doc/plans/a/PLAN-r.md",
            "# R\n\nImplements: section_01#beta\n",
        ),
    ]);
    let reported: Vec<String> = found(&project, "implements-duplicate")
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(reported, ["doc/plans/a/PLAN-p.md", "doc/plans/a/PLAN-q.md"]);
}
