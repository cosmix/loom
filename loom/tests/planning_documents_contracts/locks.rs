use super::{p, put, tmp};
use loom::planning::paths::{classify_write, DocArea, LockReason, LockedSet};
use loom::planning::slug::PlanState;
use std::fs;
use std::path::Path;

#[test]
fn locked_brief_of_running_plan_in_each_home() {
    for home in ["doc/plans", "doc/plans/demo"] {
        let (_temp, root) = tmp();
        let b = format!("{home}/briefs");
        let plan = format!("Read {b}/shared/x.md first.\n");
        put(&root, &format!("{home}/IN_PROGRESS-PLAN-run.md"), plan);
        for rel in ["run/common.md", "shared/x.md", "free/common.md"] {
            put(&root, &format!("{b}/{rel}"), "# Brief\n");
        }
        for rel in [
            "PLAN-free.md",
            "REVIEW-PLAN-free.md",
            "archive/notes.md",
            "DONE-PLAN-done.md",
            "RETIRED-PLAN-retired.md",
        ] {
            put(&root, &format!("{home}/{rel}"), "# Plan\n");
        }
        let set = LockedSet::scan(&root).unwrap();
        assert_home_locks(&set, home);
        let locked = [
            "DONE-PLAN-done.md",
            "IN_PROGRESS-PLAN-run.md",
            "RETIRED-PLAN-retired.md",
            "REVIEW-PLAN-free.md",
            "archive",
            "briefs/run/common.md",
            "briefs/shared/x.md",
        ];
        let want: Vec<_> = locked
            .iter()
            .map(|rel| p(&format!("{home}/{rel}")))
            .collect();
        assert_eq!(set.locked_paths(), want);
        assert_eq!(
            classify_write(&set, Path::new("doc/loom/knowledge/x.md")),
            DocArea::Knowledge
        );
        assert_eq!(
            classify_write(&set, Path::new("doc/loom/summaries/x.md")),
            DocArea::Summaries
        );
        assert_eq!(
            classify_write(&set, Path::new("src/main.rs")),
            DocArea::OutsideDoc
        );
    }
}

fn assert_home_locks(set: &LockedSet, home: &str) {
    let brief = Some(LockReason::BriefOfLockedPlan { slug: "run".into() });
    let cases = [
        (
            "IN_PROGRESS-PLAN-run.md",
            Some(LockReason::PlanState(PlanState::Running)),
        ),
        (
            "DONE-PLAN-done.md",
            Some(LockReason::PlanState(PlanState::Done)),
        ),
        (
            "RETIRED-PLAN-retired.md",
            Some(LockReason::PlanState(PlanState::Retired)),
        ),
        (
            "archive/DONE-PLAN-done.md",
            Some(LockReason::PlanState(PlanState::Done)),
        ),
        (
            "archive/RETIRED-PLAN-retired.md",
            Some(LockReason::PlanState(PlanState::Retired)),
        ),
        ("briefs/run/common.md", brief.clone()),
        ("briefs/shared/x.md", brief.clone()),
        ("briefs/run/new.md", brief),
        ("REVIEW-PLAN-free.md", Some(LockReason::RunReview)),
        ("archive", Some(LockReason::Archive)),
        ("archive/notes.md", Some(LockReason::Archive)),
        ("archive/new/note.md", Some(LockReason::Archive)),
        ("PLAN-free.md", None),
        ("briefs/free/common.md", None),
    ];
    for (rel, want) in cases {
        let path = p(&format!("{home}/{rel}"));
        assert_eq!(set.lock_reason(&path), want, "{}", path.display());
        let area = want.map_or(DocArea::Doc, DocArea::Locked);
        assert_eq!(classify_write(set, &path), area, "{}", path.display());
    }
}

#[test]
fn scan_locks_conflicting_plan_bundles_in_every_home() {
    let (_temp, root) = tmp();
    for home in ["doc/plans", "doc/plans/demo", "doc/plans/other"] {
        put(
            &root,
            &format!("{home}/IN_PROGRESS-PLAN-shared.md"),
            "# Shared\n",
        );
        put(
            &root,
            &format!("{home}/briefs/shared/common.md"),
            "# Brief\n",
        );
        fs::create_dir_all(root.join(home).join("archive")).unwrap();
    }
    let set = LockedSet::scan(&root).unwrap();
    let locked = set.locked_paths();
    for home in ["doc/plans", "doc/plans/demo", "doc/plans/other"] {
        let plan = p(&format!("{home}/IN_PROGRESS-PLAN-shared.md"));
        let brief = p(&format!("{home}/briefs/shared/common.md"));
        assert_eq!(
            set.lock_reason(&plan),
            Some(LockReason::PlanState(PlanState::Running))
        );
        assert_eq!(
            set.lock_reason(&brief),
            Some(LockReason::BriefOfLockedPlan {
                slug: "shared".into(),
            })
        );
        assert!(locked.contains(&plan), "{}", plan.display());
        assert!(locked.contains(&brief), "{}", brief.display());
        assert!(locked.contains(&p(&format!("{home}/archive"))), "{home}");
    }
    assert_eq!(locked.len(), 9);
}
