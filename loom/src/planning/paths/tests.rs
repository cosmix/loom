//! Tests for `planning::paths`.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::*;

fn tmp() -> TempDir {
    tempfile::Builder::new()
        .prefix("loom-planning-")
        .tempdir_in(std::env::temp_dir())
        .unwrap()
}

fn put(root: &Path, rel: &str, text: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), text).unwrap();
}

/// A repo-relative path under the brief directory, spelled so this file names no brief.
fn brief(rel: &str) -> String {
    format!("doc/plans/{}/{rel}", "briefs")
}

fn reason(set: &LockedSet, rel: &str) -> Option<LockReason> {
    set.lock_reason(Path::new(rel))
}

fn brief_of(slug: &str) -> Option<LockReason> {
    Some(LockReason::BriefOfLockedPlan { slug: slug.into() })
}

fn state(state: PlanState) -> Option<LockReason> {
    Some(LockReason::PlanState(state))
}

#[test]
fn every_lock_reason() {
    let temp = tmp();
    let root = temp.path();
    for rel in [
        "doc/plans/IN_PROGRESS-PLAN-run.md",
        "doc/plans/DONE-PLAN-done.md",
        "doc/plans/RETIRED-PLAN-old.md",
        "doc/plans/archive/PLAN-arch.md",
        "doc/plans/archive/notes.md",
        "doc/plans/REVIEW-PLAN-run.md",
    ] {
        put(root, rel, "# Plan\n");
    }
    put(root, &brief("run/common.md"), "# Brief\n");
    let set = LockedSet::scan(root).unwrap();
    for (rel, expected) in [
        ("IN_PROGRESS-PLAN-run.md", state(PlanState::Running)),
        ("DONE-PLAN-done.md", state(PlanState::Done)),
        ("RETIRED-PLAN-old.md", state(PlanState::Retired)),
        ("archive/PLAN-arch.md", state(PlanState::Done)),
        ("archive/notes.md", Some(LockReason::Archive)),
        ("archive/new/x.md", Some(LockReason::Archive)),
        ("REVIEW-PLAN-run.md", Some(LockReason::RunReview)),
    ] {
        assert_eq!(reason(&set, &format!("doc/plans/{rel}")), expected, "{rel}");
    }
    assert_eq!(reason(&set, &brief("run/common.md")), brief_of("run"));
    for slug in ["done", "old", "arch"] {
        assert_eq!(
            reason(&set, &brief(&format!("{slug}/new.md"))),
            brief_of(slug),
            "{slug}"
        );
    }
    assert_eq!(
        reason(&set, &brief("run2/x.md")),
        None,
        "a bundle prefix is per component"
    );
}

#[test]
fn names_lock_paths_that_do_not_exist_in_any_case() {
    let temp = tmp();
    let root = temp.path();
    put(root, "doc/plans/IN_PROGRESS-PLAN-run.md", "# Plan\n");
    let set = LockedSet::scan(root).unwrap();
    assert_eq!(
        reason(&set, "doc/plans/DONE-PLAN-absent.md"),
        state(PlanState::Done)
    );
    assert_eq!(
        reason(&set, "doc/plans/RETIRED-PLAN-absent.md"),
        state(PlanState::Retired)
    );
    assert_eq!(
        reason(&set, "doc/plans/REVIEW-new.md"),
        Some(LockReason::RunReview)
    );
    assert_eq!(
        reason(&set, "doc/plans/in_progress-plan-run.md"),
        state(PlanState::Running)
    );
    assert_eq!(
        reason(&set, "doc/plans/review-plan-run.md"),
        Some(LockReason::RunReview)
    );
    assert_eq!(
        reason(&set, "doc/plans/Archive/x.md"),
        Some(LockReason::Archive)
    );
    assert_eq!(reason(&set, &brief("RUN/x.md")), brief_of("run"));
    for free in [
        "doc/plans/PLAN-absent.md",
        "doc/plans/IN_PROGRESS-PLAN-.md",
        "doc/x/REVIEW-a.md",
    ] {
        assert_eq!(reason(&set, free), None, "{free}");
    }
}

#[test]
fn stray_in_progress_copy_beside_winning_done_stays_locked() {
    let temp = tmp();
    let root = temp.path();
    put(root, "doc/plans/DONE-PLAN-x.md", "# Done\n");
    let stray = format!("Read {} first.\n", brief("shared/s.md"));
    put(root, "doc/plans/IN_PROGRESS-PLAN-x.md", &stray);
    put(root, &brief("shared/s.md"), "# Shared\n");
    let tree = discover(root).unwrap();
    assert_eq!(
        tree.plan("x").unwrap().path,
        PathBuf::from("doc/plans/DONE-PLAN-x.md")
    );
    let set = LockedSet::scan(root).unwrap();
    assert_eq!(
        reason(&set, "doc/plans/IN_PROGRESS-PLAN-x.md"),
        state(PlanState::Running)
    );
    assert_eq!(
        reason(&set, "doc/plans/DONE-PLAN-x.md"),
        state(PlanState::Done)
    );
    assert_eq!(
        reason(&set, &brief("shared/s.md")),
        brief_of("x"),
        "the stray's own inputs"
    );
}

#[test]
fn brief_named_through_a_glob_is_locked() {
    let temp = tmp();
    let root = temp.path();
    let plan = format!("Read `{}` first.\n", brief("shared/*.md"));
    put(root, "doc/plans/IN_PROGRESS-PLAN-run.md", &plan);
    for rel in ["shared/a.md", "shared/b.md", "shared/c.txt"] {
        put(root, &brief(rel), "# Shared\n");
    }
    let set = LockedSet::scan(root).unwrap();
    assert_eq!(reason(&set, &brief("shared/a.md")), brief_of("run"));
    assert_eq!(reason(&set, &brief("shared/b.md")), brief_of("run"));
    assert_eq!(
        reason(&set, &brief("shared/c.txt")),
        None,
        "outside the glob"
    );
    assert_eq!(
        reason(&set, &brief("shared/new.md")),
        None,
        "not in the plan's bundle"
    );
}

#[test]
fn plan_locked_between_scans() {
    let temp = tmp();
    let root = temp.path();
    let plan = format!("Read {} first.\n", brief("shared/s.md"));
    put(root, "doc/plans/PLAN-x.md", &plan);
    put(root, &brief("shared/s.md"), "# Shared\n");
    put(root, &brief("x/common.md"), "# Common\n");
    let written = LockedSet::scan(root).unwrap();
    for rel in [
        "doc/plans/PLAN-x.md".to_string(),
        brief("shared/s.md"),
        brief("x/common.md"),
    ] {
        assert_eq!(reason(&written, &rel), None, "{rel}");
    }
    let plans = root.join("doc/plans");
    fs::rename(plans.join("PLAN-x.md"), plans.join("IN_PROGRESS-PLAN-x.md")).unwrap();
    let running = LockedSet::scan(root).unwrap();
    assert_eq!(
        reason(&running, "doc/plans/IN_PROGRESS-PLAN-x.md"),
        state(PlanState::Running)
    );
    assert_eq!(reason(&running, &brief("shared/s.md")), brief_of("x"));
    assert_eq!(reason(&running, &brief("x/common.md")), brief_of("x"));
}

#[test]
fn unreadable_locked_plan_fails_the_scan() {
    let temp = tmp();
    let root = temp.path();
    let plan = root.join("doc/plans/IN_PROGRESS-PLAN-run.md");
    put(root, "doc/plans/IN_PROGRESS-PLAN-run.md", "# Plan\n");
    fs::set_permissions(&plan, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&plan).is_ok() {
        // Running as root: permissions do not stop the read, so nothing fails.
        return;
    }
    let error = LockedSet::scan(root).unwrap_err();
    assert!(
        format!("{error:#}").contains("cannot list the inputs of locked plan"),
        "{error:#}"
    );
}

#[test]
fn locked_paths_lists_existing_files_and_the_archive_directory() {
    let temp = tmp();
    let root = temp.path();
    let plan = format!(
        "Read {} and {}.\n",
        brief("shared/x.md"),
        brief("shared/missing.md")
    );
    put(root, "doc/plans/IN_PROGRESS-PLAN-run.md", &plan);
    for rel in [
        "REVIEW-PLAN-run.md",
        "archive/DONE-PLAN-a.md",
        "archive/notes.md",
        "PLAN-free.md",
    ] {
        put(root, &format!("doc/plans/{rel}"), "# Doc\n");
    }
    for rel in ["run/common.md", "shared/x.md", "free/common.md"] {
        put(root, &brief(rel), "# Brief\n");
    }
    let set = LockedSet::scan(root).unwrap();
    let mut want: Vec<PathBuf> = [
        "doc/plans/IN_PROGRESS-PLAN-run.md".to_string(),
        "doc/plans/REVIEW-PLAN-run.md".to_string(),
        "doc/plans/archive".to_string(),
        brief("run/common.md"),
        brief("shared/x.md"),
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    want.sort();
    assert_eq!(set.locked_paths(), want);
    assert_eq!(
        reason(&set, &brief("shared/missing.md")),
        brief_of("run"),
        "named, absent"
    );
}

#[test]
fn classify_write_areas() {
    let temp = tmp();
    let set = LockedSet::scan(temp.path()).unwrap();
    assert!(set.locked_paths().is_empty(), "no doc/ locks nothing");
    let area = |rel: &str| classify_write(&set, Path::new(rel));
    assert_eq!(
        area("doc/plans/DONE-PLAN-x.md"),
        DocArea::Locked(LockReason::PlanState(PlanState::Done))
    );
    assert_eq!(area("doc/loom/knowledge/x.md"), DocArea::Knowledge);
    assert_eq!(area("doc/loom/Knowledge/x.md"), DocArea::Knowledge);
    assert_eq!(area("doc/loom/summaries/x.md"), DocArea::Summaries);
    assert_eq!(area("doc/loom/other.md"), DocArea::Doc);
    assert_eq!(area("doc/product/section_01.md"), DocArea::Doc);
    for outside in [
        "doc",
        "src/main.rs",
        "DOC/x.md",
        "/doc/x.md",
        "./doc/x.md",
        "doc/../src/x.rs",
    ] {
        assert_eq!(area(outside), DocArea::OutsideDoc, "{outside}");
    }
}

#[test]
fn relpath_syntax_rules() {
    use PathSyntaxError as E;
    let long = "a".repeat(1025);
    let wide = "é".repeat(513);
    for (raw, want) in [
        ("", E::Empty),
        ("/etc/passwd", E::Absolute),
        ("/", E::Absolute),
        ("a\\b.md", E::Backslash),
        ("a\u{0}b.md", E::ControlChar('\u{0}')),
        ("a\nb.md", E::ControlChar('\n')),
        ("a\u{1b}b.md", E::ControlChar('\u{1b}')),
        ("a\u{7f}b.md", E::ControlChar('\u{7f}')),
        ("a\u{85}b.md", E::ControlChar('\u{85}')),
        ("a\u{9f}b.md", E::ControlChar('\u{9f}')),
        (long.as_str(), E::TooLong(1025)),
        (wide.as_str(), E::TooLong(1026)),
        ("a//b.md", E::EmptyComponent),
        ("a/", E::EmptyComponent),
        ("./a.md", E::DotComponent),
        ("a/.", E::DotComponent),
        ("a/../b.md", E::DotComponent),
        ("..", E::DotComponent),
    ] {
        assert_eq!(check_relpath_syntax(raw), Err(want), "{raw:?}");
    }
    let edge = "a".repeat(1024);
    assert_eq!(
        check_relpath_syntax(&edge),
        Ok(Path::new("doc").join(&edge))
    );
    for ok in [
        "plans/PLAN-x.md",
        "a/.hidden.md",
        "a/..b.md",
        "caf\u{e9}\u{a0}.md",
    ] {
        assert_eq!(
            check_relpath_syntax(ok),
            Ok(Path::new("doc").join(ok)),
            "{ok:?}"
        );
    }
    assert!(E::ControlChar('\u{85}').to_string().contains("U+0085"));
}

#[test]
fn symlink_component_walk() {
    let temp = tmp();
    let root = temp.path();
    fs::create_dir_all(root.join("doc")).unwrap();
    fs::create_dir_all(root.join("elsewhere/briefs")).unwrap();
    symlink(root.join("elsewhere"), root.join("doc/plans")).unwrap();
    let middle = symlink_component(root, Path::new("doc/plans/briefs/x.md")).unwrap();
    assert_eq!(
        middle,
        Some(PathBuf::from("doc/plans")),
        "a link in the middle"
    );
    fs::create_dir_all(root.join("doc/product")).unwrap();
    let tail = symlink_component(root, Path::new("doc/product/new/deeper/x.md")).unwrap();
    assert_eq!(tail, None, "a missing tail cannot be a link yet");
    put(root, "doc/product/a.md", "# A\n");
    let past_file = symlink_component(root, Path::new("doc/product/a.md/x")).unwrap();
    assert_eq!(past_file, None, "nothing exists past a regular file");
    symlink(root.join("gone"), root.join("doc/product/dangling.md")).unwrap();
    let dangling = symlink_component(root, Path::new("doc/product/dangling.md")).unwrap();
    assert_eq!(
        dangling,
        Some(PathBuf::from("doc/product/dangling.md")),
        "a dangling link"
    );
    for bad in ["doc/../x", "/doc/x", "./doc/x"] {
        assert!(symlink_component(root, Path::new(bad)).is_err(), "{bad}");
    }
}

#[test]
fn lock_reason_json() {
    let cases = [
        (
            LockReason::PlanState(PlanState::Running),
            json!({"reason": "plan_state", "state": "running"}),
        ),
        (LockReason::Archive, json!({"reason": "archive"})),
        (LockReason::RunReview, json!({"reason": "run_review"})),
        (
            LockReason::BriefOfLockedPlan { slug: "x".into() },
            json!({"reason": "brief_of_locked_plan", "slug": "x"}),
        ),
    ];
    for (reason, want) in cases {
        assert_eq!(serde_json::to_value(&reason).unwrap(), want, "{reason:?}");
    }
}
