//! The rename's refusals and finishing an interrupted abandon.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use serial_test::serial;

use super::super::plan::RunningPlan;
use super::super::retire::rename_to_retired;
use super::super::{finish_abandon, stranded_plan, AbandonOutcome};
use super::{Repo, PLAN, RETIRED};
use crate::planning::runtime::RuntimePaths;

fn plan_a() -> RunningPlan {
    RunningPlan {
        slug: "a".to_string(),
        feature: None,
    }
}

/// A repository holding [`PLAN`] with no workspace, as an interrupted abandon leaves it.
fn stranded_repo() -> Repo {
    let repo = Repo::new();
    repo.put(PLAN, "# A\n");
    repo.commit_all("plan a");
    repo
}

fn write_marker(repo: &Repo, runtime: &RuntimePaths) {
    let marker = repo.marker(runtime);
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let body = format!("{{\"plan\":\"{PLAN}\",\"started_at\":\"2026-10-29T00:00:00Z\"}}");
    fs::write(marker, body).unwrap();
}

fn assert_refused(outcome: &AbandonOutcome) {
    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{outcome:?}"
    );
}

#[test]
#[serial]
fn an_existing_target_refuses_the_rename() {
    let repo = stranded_repo();
    repo.put(RETIRED, "# Older\n");
    assert_refused(&rename_to_retired(&repo.root, &plan_a()).unwrap());
    assert!(repo.has(PLAN));
    assert_eq!(
        fs::read_to_string(repo.root.join(RETIRED)).unwrap(),
        "# Older\n"
    );
}

#[test]
#[serial]
fn a_symlinked_plan_file_refuses_the_rename() {
    let repo = Repo::new();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("plan.md");
    fs::write(&target, "# Elsewhere\n").unwrap();
    fs::create_dir_all(repo.root.join("doc/plans")).unwrap();
    symlink(&target, repo.root.join(PLAN)).unwrap();
    assert_refused(&rename_to_retired(&repo.root, &plan_a()).unwrap());
    assert_eq!(fs::read_to_string(&target).unwrap(), "# Elsewhere\n");
    assert!(!repo.root.join("doc/plans/archive").exists());
}

#[test]
#[serial]
fn a_symlinked_archive_refuses_the_rename() {
    let repo = stranded_repo();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), repo.root.join("doc/plans/archive")).unwrap();
    assert_refused(&rename_to_retired(&repo.root, &plan_a()).unwrap());
    assert!(repo.has(PLAN));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
#[serial]
fn a_symlinked_plans_directory_refuses_the_rename() {
    let repo = Repo::new();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("IN_PROGRESS-PLAN-a.md"), "# A\n").unwrap();
    fs::create_dir_all(repo.root.join("doc")).unwrap();
    symlink(outside.path(), repo.root.join("doc/plans")).unwrap();
    assert_refused(&rename_to_retired(&repo.root, &plan_a()).unwrap());
    let entries: Vec<_> = fs::read_dir(outside.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(outside.path().join("IN_PROGRESS-PLAN-a.md").is_file());
}

#[test]
#[serial]
fn finish_abandon_refuses_a_leftover_branch_then_renames() {
    let repo = stranded_repo();
    let runtime = repo.runtime();
    write_marker(&repo, &runtime);
    repo.git(&["branch", "loom/s1"]);
    assert_eq!(stranded_plan(&repo.root, &runtime), None);
    let outcome = finish_abandon(&repo.root, &runtime).unwrap();
    match &outcome {
        AbandonOutcome::Refused { reason } => assert!(reason.contains("loom/s1"), "{reason}"),
        other => panic!("expected Refused, got {other:?}"),
    }
    assert!(repo.has(PLAN));

    repo.git(&["branch", "-D", "-q", "loom/s1"]);
    assert_eq!(stranded_plan(&repo.root, &runtime).as_deref(), Some(PLAN));
    let outcome = finish_abandon(&repo.root, &runtime).unwrap();
    assert_eq!(
        outcome,
        AbandonOutcome::Renamed {
            to: PathBuf::from(RETIRED)
        }
    );
    assert!(repo.has(RETIRED));
    assert!(
        !repo.marker(&runtime).exists(),
        "the marker outlived the rename"
    );
}

#[test]
#[serial]
fn finish_abandon_refuses_a_leftover_worktree_directory() {
    let repo = stranded_repo();
    let runtime = repo.runtime();
    write_marker(&repo, &runtime);
    fs::create_dir_all(repo.root.join(".worktrees/s1")).unwrap();
    assert_eq!(stranded_plan(&repo.root, &runtime), None);
    assert_refused(&finish_abandon(&repo.root, &runtime).unwrap());
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn a_stranded_tree_without_the_marker_is_not_offered() {
    let repo = stranded_repo();
    let runtime = repo.runtime();
    assert_eq!(stranded_plan(&repo.root, &runtime), None);
    assert_refused(&finish_abandon(&repo.root, &runtime).unwrap());
    assert!(repo.has(PLAN));
    assert!(!repo.root.join(RETIRED).exists());
}

#[test]
#[serial]
fn a_marker_naming_another_plan_is_not_offered() {
    let repo = stranded_repo();
    let runtime = repo.runtime();
    let marker = repo.marker(&runtime);
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let body = r#"{"plan":"doc/plans/IN_PROGRESS-PLAN-b.md","started_at":"2026-10-29T00:00:00Z"}"#;
    fs::write(marker, body).unwrap();
    assert_eq!(stranded_plan(&repo.root, &runtime), None);
}
