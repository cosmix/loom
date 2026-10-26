//! Tests for `planning::slug`.

use std::path::{Path, PathBuf};

use crate::planning::slug::{plan_slug, plan_state, resolve_plans, PlanState};

#[test]
fn slug_strips_one_prefix_and_requires_a_stem() {
    for (name, expected) in [
        ("PLAN-a.md", Some("a")),
        ("IN_PROGRESS-PLAN-a.md", Some("a")),
        ("DONE-PLAN-a.md", Some("a")),
        ("RETIRED-PLAN-a.md", Some("a")),
        ("DONE-PLAN-DONE-x.md", Some("DONE-x")),
        ("DONE-RETIRED-PLAN-a.md", None),
        ("PLAN-.md", None),
        ("REVIEW-PLAN-a.md", None),
        ("PLAN-a.txt", None),
    ] {
        assert_eq!(plan_slug(name).as_deref(), expected, "{name}");
    }
}

#[test]
fn state_recognizes_only_direct_plan_and_archive_paths() {
    for (path, expected) in [
        ("doc/plans/PLAN-a.md", Some(PlanState::Written)),
        ("doc/plans/IN_PROGRESS-PLAN-a.md", Some(PlanState::Running)),
        ("doc/plans/DONE-PLAN-a.md", Some(PlanState::Done)),
        ("doc/plans/RETIRED-PLAN-a.md", Some(PlanState::Retired)),
        ("doc/plans/archive/PLAN-a.md", Some(PlanState::Done)),
        (
            "doc/plans/archive/IN_PROGRESS-PLAN-a.md",
            Some(PlanState::Done),
        ),
        ("doc/plans/archive/DONE-PLAN-a.md", Some(PlanState::Done)),
        (
            "doc/plans/archive/RETIRED-PLAN-a.md",
            Some(PlanState::Retired),
        ),
        ("doc/plans/archive/nested/PLAN-a.md", None),
        ("doc/plans/briefs/PLAN-a.md", None),
        ("plans/PLAN-a.md", None),
        ("doc/plans/PLAN-.md", None),
    ] {
        assert_eq!(plan_state(Path::new(path)), expected, "{path}");
    }
}

#[test]
fn resolution_selects_highest_state_and_sorts_strays() {
    let paths = [
        "doc/plans/DONE-PLAN-a.md",
        "doc/plans/PLAN-a.md",
        "doc/plans/archive/RETIRED-PLAN-a.md",
        "doc/plans/IN_PROGRESS-PLAN-a.md",
        "doc/plans/briefs/PLAN-a.md",
    ];
    let plans = resolve_plans(paths.into_iter().map(PathBuf::from));
    let plan = &plans["a"];
    assert_eq!(plan.slug, "a");
    assert_eq!(plan.state, PlanState::Retired);
    assert_eq!(
        plan.path,
        PathBuf::from("doc/plans/archive/RETIRED-PLAN-a.md")
    );
    assert_eq!(
        plan.strays,
        [
            PathBuf::from("doc/plans/DONE-PLAN-a.md"),
            PathBuf::from("doc/plans/IN_PROGRESS-PLAN-a.md"),
            PathBuf::from("doc/plans/PLAN-a.md"),
        ]
    );
}

#[test]
fn resolution_breaks_done_ties_by_path_in_either_order() {
    let smaller = PathBuf::from("doc/plans/DONE-PLAN-a.md");
    let larger = PathBuf::from("doc/plans/archive/PLAN-a.md");
    for paths in [
        [larger.clone(), smaller.clone()],
        [smaller.clone(), larger.clone()],
    ] {
        let plans = resolve_plans(paths.into_iter());
        assert_eq!(plans["a"].path, smaller);
        assert_eq!(plans["a"].state, PlanState::Done);
        assert_eq!(plans["a"].strays, std::slice::from_ref(&larger));
    }
}
