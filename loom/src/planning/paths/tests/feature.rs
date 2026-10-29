//! Feature homes share the standalone shelf's locking rules.

use super::*;

#[test]
fn feature_plans_archives_reviews_and_bundles_are_locked() {
    let temp = tmp();
    let root = temp.path();
    for rel in [
        "feature/IN_PROGRESS-PLAN-run.md",
        "feature/DONE-PLAN-done.md",
        "feature/RETIRED-PLAN-old.md",
        "feature/archive/DONE-PLAN-arch.md",
        "feature/REVIEW-PLAN-run.md",
    ] {
        put(root, &format!("doc/plans/{rel}"), "# Plan\n");
    }
    put(root, "doc/plans/feature/briefs/run/common.md", "# Brief\n");
    fs::create_dir_all(root.join("doc/plans/empty/archive")).unwrap();

    let set = LockedSet::scan(root).unwrap();
    for (rel, expected) in [
        ("IN_PROGRESS-PLAN-run.md", state(PlanState::Running)),
        ("DONE-PLAN-done.md", state(PlanState::Done)),
        ("RETIRED-PLAN-old.md", state(PlanState::Retired)),
        ("archive/DONE-PLAN-arch.md", state(PlanState::Done)),
        ("archive/new/x.md", Some(LockReason::Archive)),
        ("REVIEW-PLAN-run.md", Some(LockReason::RunReview)),
        ("briefs/run/common.md", brief_of("run")),
        ("briefs/arch/new.md", brief_of("arch")),
    ] {
        let path = format!("doc/plans/feature/{rel}");
        assert_eq!(reason(&set, &path), expected, "{path}");
    }
    let want = [
        "doc/plans/empty/archive",
        "doc/plans/feature/DONE-PLAN-done.md",
        "doc/plans/feature/IN_PROGRESS-PLAN-run.md",
        "doc/plans/feature/RETIRED-PLAN-old.md",
        "doc/plans/feature/REVIEW-PLAN-run.md",
        "doc/plans/feature/archive",
        "doc/plans/feature/briefs/run/common.md",
    ]
    .map(PathBuf::from);
    assert_eq!(set.locked_paths(), want);
}

#[test]
fn feature_write_classification_folds_names_and_allows_new_documents() {
    let temp = tmp();
    put(
        temp.path(),
        "doc/plans/feature/IN_PROGRESS-PLAN-run.md",
        "# Plan\n",
    );
    let set = LockedSet::scan(temp.path()).unwrap();
    for (rel, expected) in [
        (
            "in_progress-plan-new.md",
            LockReason::PlanState(PlanState::Running),
        ),
        ("DONE-PLAN-new.md", LockReason::PlanState(PlanState::Done)),
        (
            "retired-plan-new.md",
            LockReason::PlanState(PlanState::Retired),
        ),
        ("Archive/new.md", LockReason::Archive),
        ("review-new.md", LockReason::RunReview),
        (
            "BRIEFS/RUN/new.md",
            LockReason::BriefOfLockedPlan { slug: "run".into() },
        ),
    ] {
        let path = PathBuf::from(format!("doc/plans/FEATURE/{rel}"));
        assert_eq!(classify_write(&set, &path), DocArea::Locked(expected));
    }
    for rel in [
        "doc/plans/feature/PLAN-new.md",
        "doc/plans/feature/index.md",
        "doc/plans/feature/metaplan-02.md",
        "doc/plans/feature/briefs/run2/common.md",
        "doc/plans/other/briefs/run/common.md",
        "doc/product/feature/index.md",
        "doc/product/feature/section_01.md",
    ] {
        assert_eq!(classify_write(&set, Path::new(rel)), DocArea::Doc, "{rel}");
    }
}

#[test]
fn invalid_feature_names_do_not_add_locks() {
    let temp = tmp();
    let root = temp.path();
    for rel in [
        "IN_PROGRESS-PLAN-bad.md",
        "REVIEW-PLAN-bad.md",
        "archive/DONE-PLAN-bad.md",
        "briefs/bad/common.md",
    ] {
        put(root, &format!("doc/plans/bad_name/{rel}"), "# Document\n");
    }
    // If an invalid feature were scanned as a locked plan, this read would fail closed.
    fs::write(
        root.join("doc/plans/bad_name/IN_PROGRESS-PLAN-bad.md"),
        [0xff],
    )
    .unwrap();

    let set = LockedSet::scan(root).unwrap();
    assert!(set.locked_paths().is_empty());
    assert_eq!(reason(&set, "doc/plans/bad_name/briefs/bad/new.md"), None);
    assert_eq!(reason(&set, "doc/plans/bad_name/archive/new.md"), None);
}

#[test]
fn conflicting_locked_plans_keep_each_homes_bundle_locked() {
    let temp = tmp();
    for rel in [
        "doc/plans/DONE-PLAN-x.md",
        "doc/plans/feature/IN_PROGRESS-PLAN-x.md",
        "doc/plans/another/archive/RETIRED-PLAN-x.md",
    ] {
        put(temp.path(), rel, "# Plan\n");
    }

    let set = LockedSet::scan(temp.path()).unwrap();
    for rel in [
        "doc/plans/briefs/x/new.md",
        "doc/plans/feature/briefs/x/new.md",
        "doc/plans/another/briefs/x/new.md",
    ] {
        assert_eq!(reason(&set, rel), brief_of("x"), "{rel}");
    }
}

#[test]
fn unreadable_feature_locked_plan_fails_the_scan() {
    let temp = tmp();
    let rel = "doc/plans/feature/IN_PROGRESS-PLAN-run.md";
    put(temp.path(), rel, "# Plan\n");
    fs::write(temp.path().join(rel), [0xff]).unwrap();

    let error = LockedSet::scan(temp.path()).unwrap_err();
    assert!(format!("{error:#}").contains(&format!("cannot list the inputs of locked plan {rel}")));
}

#[test]
fn feature_bundle_inputs_follow_recursive_references() {
    let temp = tmp();
    let root = temp.path();
    put(
        root,
        "doc/plans/feature/IN_PROGRESS-PLAN-run.md",
        "# Plan\n",
    );
    let shared = "doc/plans/feature/briefs/shared/notes.md";
    put(
        root,
        "doc/plans/feature/briefs/run/common.md",
        &format!("Read `{shared}`.\n"),
    );
    put(root, shared, "# Shared\n");

    let set = LockedSet::scan(root).unwrap();
    assert_eq!(reason(&set, shared), brief_of("run"));
    assert_eq!(reason(&set, "doc/plans/briefs/run/new.md"), None);
}
