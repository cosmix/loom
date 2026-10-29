//! Tests for `planning::slug`.

use std::path::{Path, PathBuf};

use crate::planning::slug::{
    plan_feature, plan_home, plan_slug, plan_state, resolve_plans, PlanEntry, PlanState,
};

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
fn state_recognizes_standalone_plan_and_archive_paths() {
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
        ("doc/plans/Bad/PLAN-a.md", None),
        ("doc/plans/feature/sub/PLAN-a.md", None),
        ("doc/plans/feature/briefs/PLAN-a.md", None),
        ("plans/PLAN-a.md", None),
        ("doc/plans/PLAN-.md", None),
    ] {
        assert_eq!(plan_state(Path::new(path)), expected, "{path}");
    }
}

#[test]
fn state_recognizes_feature_plan_and_archive_paths() {
    for (path, expected) in [
        ("doc/plans/feature/PLAN-a.md", PlanState::Written),
        (
            "doc/plans/feature/IN_PROGRESS-PLAN-a.md",
            PlanState::Running,
        ),
        ("doc/plans/feature/DONE-PLAN-a.md", PlanState::Done),
        ("doc/plans/feature/RETIRED-PLAN-a.md", PlanState::Retired),
        ("doc/plans/feature/archive/PLAN-a.md", PlanState::Done),
        (
            "doc/plans/feature/archive/IN_PROGRESS-PLAN-a.md",
            PlanState::Done,
        ),
        ("doc/plans/feature/archive/DONE-PLAN-a.md", PlanState::Done),
        (
            "doc/plans/feature/archive/RETIRED-PLAN-a.md",
            PlanState::Retired,
        ),
        ("doc/plans/0-feature/PLAN-a.md", PlanState::Written),
    ] {
        assert_eq!(plan_state(Path::new(path)), Some(expected), "{path}");
    }
}

#[test]
fn plan_location_reports_home_and_feature() {
    for (path, home, feature) in [
        ("doc/plans/PLAN-a.md", "doc/plans", None),
        ("doc/plans/archive/PLAN-a.md", "doc/plans", None),
        (
            "doc/plans/feature/PLAN-a.md",
            "doc/plans/feature",
            Some("feature"),
        ),
        (
            "doc/plans/feature/archive/RETIRED-PLAN-a.md",
            "doc/plans/feature",
            Some("feature"),
        ),
    ] {
        let rel = Path::new(path);
        assert_eq!(plan_home(rel), Some(PathBuf::from(home)), "{path}");
        assert_eq!(plan_feature(rel).as_deref(), feature, "{path}");
    }
}

#[test]
fn plan_location_rejects_invalid_homes_and_non_plans() {
    for path in [
        "doc/plans/briefs/PLAN-x.md",
        "doc/plans/briefs/archive/PLAN-x.md",
        "doc/plans/Bad/PLAN-x.md",
        "doc/plans/Bad/archive/PLAN-x.md",
        "doc/plans/-bad/PLAN-x.md",
        "doc/plans/bad_name/PLAN-x.md",
        "doc/plans/feature/sub/PLAN-x.md",
        "doc/plans/feature/briefs/PLAN-x.md",
        "doc/plans/feature/archive/nested/PLAN-x.md",
        "doc/plans/archive/archive/PLAN-x.md",
        "doc/plans/archive/feature/PLAN-x.md",
        "doc/plans/feature/archive/archive/PLAN-x.md",
        "doc/plans/feature/index.md",
        "doc/plans/feature/PLAN-.md",
        "doc/plans/feature/PLAN-x.txt",
        "doc/product/feature/PLAN-x.md",
        "/doc/plans/feature/PLAN-x.md",
    ] {
        let rel = Path::new(path);
        assert_eq!(plan_state(rel), None, "{path}");
        assert_eq!(plan_home(rel), None, "{path}");
        assert_eq!(plan_feature(rel), None, "{path}");
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
    assert_eq!(plan.feature, None);
    assert!(plan.conflicts.is_empty());
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
        assert_eq!(plans["a"].feature, None);
        assert!(plans["a"].conflicts.is_empty());
    }
}

#[test]
fn resolution_keeps_feature_lifecycle_copies_as_strays() {
    let paths = [
        "doc/plans/feature/PLAN-a.md",
        "doc/plans/feature/archive/PLAN-a.md",
        "doc/plans/feature/IN_PROGRESS-PLAN-a.md",
        "doc/plans/feature/DONE-PLAN-a.md",
    ];

    let plans = resolve_plans(paths.into_iter().map(PathBuf::from));

    assert_eq!(
        plans["a"],
        PlanEntry {
            slug: "a".into(),
            path: "doc/plans/feature/DONE-PLAN-a.md".into(),
            state: PlanState::Done,
            feature: Some("feature".into()),
            strays: vec![
                "doc/plans/feature/IN_PROGRESS-PLAN-a.md".into(),
                "doc/plans/feature/PLAN-a.md".into(),
                "doc/plans/feature/archive/PLAN-a.md".into(),
            ],
            conflicts: vec![],
        }
    );
}

#[test]
fn resolution_partitions_against_final_winner_home_in_either_order() {
    let paths = [
        "doc/plans/alpha/PLAN-a.md",
        "doc/plans/alpha/IN_PROGRESS-PLAN-a.md",
        "doc/plans/beta/PLAN-a.md",
        "doc/plans/beta/archive/RETIRED-PLAN-a.md",
        "doc/plans/beta/DONE-PLAN-a.md",
        "doc/plans/alpha/DONE-PLAN-a.md",
    ];
    for paths in [paths.to_vec(), paths.into_iter().rev().collect()] {
        let plans = resolve_plans(paths.into_iter().map(PathBuf::from));

        assert_eq!(
            plans["a"],
            PlanEntry {
                slug: "a".into(),
                path: "doc/plans/beta/archive/RETIRED-PLAN-a.md".into(),
                state: PlanState::Retired,
                feature: Some("beta".into()),
                strays: vec![
                    "doc/plans/beta/DONE-PLAN-a.md".into(),
                    "doc/plans/beta/PLAN-a.md".into(),
                ],
                conflicts: vec![
                    "doc/plans/alpha/DONE-PLAN-a.md".into(),
                    "doc/plans/alpha/IN_PROGRESS-PLAN-a.md".into(),
                    "doc/plans/alpha/PLAN-a.md".into(),
                ],
            }
        );
    }
}

#[test]
fn resolution_breaks_cross_feature_state_ties_by_path_in_either_order() {
    let smaller = PathBuf::from("doc/plans/alpha/PLAN-a.md");
    let larger = PathBuf::from("doc/plans/beta/PLAN-a.md");
    for paths in [
        [larger.clone(), smaller.clone()],
        [smaller.clone(), larger.clone()],
    ] {
        let plans = resolve_plans(paths.into_iter());

        assert_eq!(plans["a"].path, smaller);
        assert_eq!(plans["a"].state, PlanState::Written);
        assert_eq!(plans["a"].feature.as_deref(), Some("alpha"));
        assert!(plans["a"].strays.is_empty());
        assert_eq!(plans["a"].conflicts, std::slice::from_ref(&larger));
    }
}

#[test]
fn resolution_reports_standalone_copy_as_feature_conflict() {
    let paths = [
        "doc/plans/PLAN-a.md",
        "doc/plans/feature/IN_PROGRESS-PLAN-a.md",
    ];

    let plans = resolve_plans(paths.into_iter().map(PathBuf::from));

    assert_eq!(
        plans["a"].path,
        PathBuf::from("doc/plans/feature/IN_PROGRESS-PLAN-a.md")
    );
    assert_eq!(plans["a"].state, PlanState::Running);
    assert_eq!(plans["a"].feature.as_deref(), Some("feature"));
    assert!(plans["a"].strays.is_empty());
    assert_eq!(plans["a"].conflicts, [PathBuf::from("doc/plans/PLAN-a.md")]);
}

#[test]
fn resolution_reports_feature_copy_as_standalone_conflict_on_tie() {
    let paths = [
        "doc/plans/feature/DONE-PLAN-a.md",
        "doc/plans/DONE-PLAN-a.md",
    ];

    let plans = resolve_plans(paths.into_iter().map(PathBuf::from));

    assert_eq!(plans["a"].path, PathBuf::from("doc/plans/DONE-PLAN-a.md"));
    assert_eq!(plans["a"].state, PlanState::Done);
    assert_eq!(plans["a"].feature, None);
    assert!(plans["a"].strays.is_empty());
    assert_eq!(
        plans["a"].conflicts,
        [PathBuf::from("doc/plans/feature/DONE-PLAN-a.md")]
    );
}
