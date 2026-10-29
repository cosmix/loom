use super::{p, put, tmp};
use loom::planning::discover::discover;
use loom::planning::project::Project;
use loom::planning::slug::{plan_feature, plan_home, plan_state, PlanState};
use std::path::Path;

#[test]
fn plan_slug_most_advanced_wins() {
    for (home, feature) in [("doc/plans", None), ("doc/plans/demo", Some("demo"))] {
        let (_temp, root) = tmp();
        let files = [
            "PLAN-foo",
            "IN_PROGRESS-PLAN-foo",
            "archive/DONE-PLAN-foo",
            "archive/RETIRED-PLAN-bar",
            "PLAN-bar",
        ];
        for name in files {
            put(&root, &format!("{home}/{name}.md"), "# Plan\n");
        }
        let tree = discover(&root).unwrap();
        let foo = tree.plan("foo").expect("plan foo");
        assert_eq!((foo.slug.as_str(), foo.state), ("foo", PlanState::Done));
        assert_eq!(foo.feature.as_deref(), feature);
        assert_eq!(foo.path, p(&format!("{home}/archive/DONE-PLAN-foo.md")));
        assert_eq!(
            foo.strays,
            [
                p(&format!("{home}/IN_PROGRESS-PLAN-foo.md")),
                p(&format!("{home}/PLAN-foo.md")),
            ]
        );
        assert!(foo.conflicts.is_empty());
        assert_eq!(tree.by_id("plan/foo").unwrap().path, foo.path);
        let bar = tree.plan("bar").expect("plan bar");
        assert_eq!(bar.state, PlanState::Retired);
        assert_eq!(bar.feature.as_deref(), feature);
        assert_eq!(bar.path, p(&format!("{home}/archive/RETIRED-PLAN-bar.md")));
        assert_eq!(bar.strays, [p(&format!("{home}/PLAN-bar.md"))]);
        assert!(bar.conflicts.is_empty());
    }
}

#[test]
fn lifecycle_state_and_home_follow_location() {
    for (home, feature) in [("doc/plans", None), ("doc/plans/demo", Some("demo"))] {
        for (name, state) in [
            ("PLAN-x.md", PlanState::Written),
            ("IN_PROGRESS-PLAN-x.md", PlanState::Running),
            ("DONE-PLAN-x.md", PlanState::Done),
            ("RETIRED-PLAN-x.md", PlanState::Retired),
            ("archive/PLAN-x.md", PlanState::Done),
            ("archive/IN_PROGRESS-PLAN-x.md", PlanState::Done),
            ("archive/DONE-PLAN-x.md", PlanState::Done),
            ("archive/RETIRED-PLAN-x.md", PlanState::Retired),
        ] {
            let path = p(&format!("{home}/{name}"));
            assert_eq!(plan_state(&path), Some(state), "{}", path.display());
            assert_eq!(plan_home(&path), Some(p(home)));
            assert_eq!(plan_feature(&path).as_deref(), feature);
        }
    }
    for rel in [
        "doc/plans/demo/sub/PLAN-x.md",
        "doc/plans/Bad/PLAN-x.md",
        "doc/plans/briefs/PLAN-x.md",
        "doc/plans/demo/notes.md",
        "elsewhere/PLAN-x.md",
    ] {
        assert_eq!(plan_state(Path::new(rel)), None, "{rel}");
        assert_eq!(plan_home(Path::new(rel)), None, "{rel}");
        assert_eq!(plan_feature(Path::new(rel)), None, "{rel}");
    }
}

#[test]
fn duplicate_slug_across_features_is_reported_separately_from_strays() {
    let (_temp, root) = tmp();
    let winner = "doc/plans/demo/archive/DONE-PLAN-shared.md";
    let stray = "doc/plans/demo/PLAN-shared.md";
    let conflict = "doc/plans/other/IN_PROGRESS-PLAN-shared.md";
    for rel in [winner, stray, conflict] {
        put(&root, rel, "# Shared\nImplements: none\n");
    }
    let project = Project::load(&root).unwrap();
    let entry = project.tree.plan("shared").unwrap();
    assert_eq!(entry.path, p(winner));
    assert_eq!(entry.feature.as_deref(), Some("demo"));
    assert_eq!(entry.strays, [p(stray)]);
    assert_eq!(entry.conflicts, [p(conflict)]);
    assert_eq!(project.tree.by_id("plan/shared").unwrap().path, p(winner));
    assert_eq!(
        copy_problems(&project),
        [
            ("stray-plan-copy", p(stray)),
            ("duplicate-plan-slug", p(conflict)),
        ]
    );
}

#[test]
fn duplicate_slug_between_standalone_and_feature_is_reported_even_when_locked() {
    let (_temp, root) = tmp();
    let winner = "doc/plans/archive/RETIRED-PLAN-shared.md";
    let conflict = "doc/plans/demo/archive/DONE-PLAN-shared.md";
    for rel in [winner, conflict] {
        put(&root, rel, "# Shared\nImplements: none\n");
    }
    let project = Project::load(&root).unwrap();
    let entry = project.tree.plan("shared").unwrap();
    assert_eq!(entry.path, p(winner));
    assert_eq!(entry.feature, None);
    assert!(entry.strays.is_empty());
    assert_eq!(entry.conflicts, [p(conflict)]);
    assert_eq!(
        copy_problems(&project),
        [("duplicate-plan-slug", p(conflict))]
    );
}

fn copy_problems(project: &Project) -> Vec<(&str, std::path::PathBuf)> {
    project
        .problems
        .iter()
        .filter(|problem| matches!(problem.code, "stray-plan-copy" | "duplicate-plan-slug"))
        .map(|problem| (problem.code, problem.path.clone()))
        .collect()
}
