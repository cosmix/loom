//! Feature and standalone Implements declarations.

use super::{found, plan, tree, vision, Path, Project, VISION};

#[test]
fn implements_numbers_are_valid_in_their_own_feature() {
    let temp = tree(&[
        (VISION, vision(&[1])),
        ("doc/product/b/index.md", vision(&[2])),
        ("doc/plans/a/PLAN-one.md", plan("section_01")),
        ("doc/plans/b/PLAN-two.md", plan("section_02")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-broken", "implements-missing"]).is_empty());
}

#[test]
fn implements_cannot_name_a_section_from_another_feature() {
    let temp = tree(&[
        (VISION, vision(&[5])),
        ("doc/product/b/index.md", vision(&[1])),
        ("doc/plans/b/PLAN-wrong.md", plan("section_05")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [(
        "implements-broken",
        Path::new("doc/plans/b/PLAN-wrong.md"),
        Some(2),
    )];
    assert_eq!(found(&project, &["implements-broken"]), want);
}

#[test]
fn plans_in_the_same_feature_cannot_implement_the_same_section() {
    let temp = tree(&[
        (VISION, vision(&[5])),
        ("doc/plans/a/PLAN-one.md", plan("section_05")),
        (
            "doc/plans/a/PLAN-two.md",
            "# Two\n\nImplements: section_05\n".to_string(),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [
        (
            "implements-duplicate",
            Path::new("doc/plans/a/PLAN-one.md"),
            Some(2),
        ),
        (
            "implements-duplicate",
            Path::new("doc/plans/a/PLAN-two.md"),
            Some(3),
        ),
    ];
    assert_eq!(found(&project, &["implements-duplicate"]), want);
}

#[test]
fn plans_in_different_features_can_implement_the_same_section_number() {
    let temp = tree(&[
        (VISION, vision(&[5])),
        ("doc/product/b/index.md", vision(&[5])),
        ("doc/plans/a/PLAN-one.md", plan("section_05")),
        ("doc/plans/b/PLAN-two.md", plan("section_05")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-duplicate", "implements-broken"]).is_empty());
}

#[test]
fn feature_plans_require_an_implements_line() {
    let temp = tree(&[("doc/plans/a/PLAN-missing.md", "# Missing\n")]);
    let project = Project::load(temp.path()).unwrap();
    let want = [(
        "implements-missing",
        Path::new("doc/plans/a/PLAN-missing.md"),
        None,
    )];
    assert_eq!(found(&project, &["implements-missing"]), want);
}

#[test]
fn feature_plans_can_explicitly_implement_none_without_a_vision() {
    let temp = tree(&[("doc/plans/a/PLAN-engineering.md", plan("none"))]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-missing", "implements-broken"]).is_empty());
}

#[test]
fn standalone_plans_may_omit_the_implements_line() {
    let temp = tree(&[("doc/plans/PLAN-missing.md", "# Missing\n")]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-missing", "implements-broken"]).is_empty());
}

#[test]
fn standalone_plans_may_explicitly_implement_none() {
    let temp = tree(&[("doc/plans/PLAN-standalone.md", plan("none"))]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-missing", "implements-broken"]).is_empty());
}

#[test]
fn standalone_plans_cannot_name_sections_even_when_a_feature_lists_them() {
    let temp = tree(&[
        (VISION, vision(&[5])),
        ("doc/plans/PLAN-standalone.md", plan("section_05")),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [(
        "implements-broken",
        Path::new("doc/plans/PLAN-standalone.md"),
        Some(2),
    )];
    assert_eq!(found(&project, &["implements-broken"]), want);
    assert!(project
        .problems
        .iter()
        .any(|p| { p.code == "implements-broken" && p.message.contains("Implements: none") }));
}

#[test]
fn locked_feature_plans_do_not_report_implements_problems() {
    let temp = tree(&[
        (VISION, vision(&[1])),
        (
            "doc/plans/a/IN_PROGRESS-PLAN-missing.md",
            "# Missing\n".to_string(),
        ),
        (
            "doc/plans/a/archive/DONE-PLAN-broken.md",
            plan("section_99"),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    assert!(found(&project, &["implements-missing", "implements-broken"]).is_empty());
}

#[test]
fn duplicate_implementations_only_report_on_unlocked_plans() {
    let temp = tree(&[
        (VISION, vision(&[1])),
        ("doc/plans/a/PLAN-new.md", plan("section_01")),
        (
            "doc/plans/a/IN_PROGRESS-PLAN-running.md",
            plan("section_01"),
        ),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let want = [(
        "implements-duplicate",
        Path::new("doc/plans/a/PLAN-new.md"),
        Some(2),
    )];
    assert_eq!(found(&project, &["implements-duplicate"]), want);
}
