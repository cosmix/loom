//! Running plan identity and paths in standalone and feature homes.

use std::path::{Path, PathBuf};

use super::super::plan::RunningPlan;

#[test]
fn from_rel_accepts_a_standalone_running_plan() {
    let source = Path::new("doc/plans/IN_PROGRESS-PLAN-a.md");

    let plan = RunningPlan::from_rel(source).unwrap();

    assert_eq!(
        plan,
        RunningPlan {
            slug: "a".to_string(),
            feature: None,
        }
    );
    assert_eq!(plan.source(), source);
    assert_eq!(
        plan.retired(),
        PathBuf::from("doc/plans/archive/RETIRED-PLAN-a.md")
    );
}

#[test]
fn from_rel_accepts_a_feature_running_plan() {
    let source = Path::new("doc/plans/my-feature/IN_PROGRESS-PLAN-a.md");

    let plan = RunningPlan::from_rel(source).unwrap();

    assert_eq!(
        plan,
        RunningPlan {
            slug: "a".to_string(),
            feature: Some("my-feature".to_string()),
        }
    );
    assert_eq!(plan.source(), source);
    assert_eq!(
        plan.source_string(),
        "doc/plans/my-feature/IN_PROGRESS-PLAN-a.md"
    );
    assert_eq!(
        plan.retired(),
        PathBuf::from("doc/plans/my-feature/archive/RETIRED-PLAN-a.md")
    );
}

#[test]
fn from_rel_rejects_non_running_plans_and_paths_outside_plan_homes() {
    for rel in [
        "doc/plans/PLAN-a.md",
        "doc/plans/DONE-PLAN-a.md",
        "doc/plans/archive/RETIRED-PLAN-a.md",
        "doc/plans/my-feature/PLAN-a.md",
        "doc/plans/my-feature/DONE-PLAN-a.md",
        "doc/plans/my-feature/archive/IN_PROGRESS-PLAN-a.md",
        "doc/plans/my-feature/archive/RETIRED-PLAN-a.md",
        "doc/plans/briefs/IN_PROGRESS-PLAN-a.md",
        "doc/plans/Bad-Feature/IN_PROGRESS-PLAN-a.md",
        "doc/plans/my-feature/nested/IN_PROGRESS-PLAN-a.md",
        "elsewhere/IN_PROGRESS-PLAN-a.md",
        "/doc/plans/IN_PROGRESS-PLAN-a.md",
    ] {
        assert_eq!(RunningPlan::from_rel(Path::new(rel)), None, "{rel}");
    }
}
