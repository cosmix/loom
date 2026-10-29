//! Abandoning and recovering feature plans keeps their own archive and path identity.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use serial_test::serial;

use super::super::marker;
use super::super::plan::RunningPlan;
use super::super::retire::rename_to_retired;
use super::super::{finish_abandon, stranded_plan, AbandonOutcome};
use super::{Repo, CLEAN_REMOVES_WORK, PLAN, RETIRED};

const FEATURE_PLAN: &str = "doc/plans/my-feature/IN_PROGRESS-PLAN-a.md";
const FEATURE_RETIRED: &str = "doc/plans/my-feature/archive/RETIRED-PLAN-a.md";

fn feature_plan() -> RunningPlan {
    RunningPlan::from_rel(Path::new(FEATURE_PLAN)).unwrap()
}

#[test]
#[serial]
fn a_feature_sequence_retires_into_its_own_created_archive() {
    let repo = Repo::new();
    repo.put(FEATURE_PLAN, "# Feature A\n");
    repo.commit_all("feature plan a");
    repo.workspace(FEATURE_PLAN, "main");
    repo.on("clean", CLEAN_REMOVES_WORK);
    let runtime = repo.runtime();

    let (outcome, lines) = repo.abandon_lines(&runtime);

    assert_eq!(
        outcome,
        AbandonOutcome::Renamed {
            to: PathBuf::from(FEATURE_RETIRED)
        }
    );
    assert_eq!(
        fs::read_to_string(repo.root.join(FEATURE_RETIRED)).unwrap(),
        "# Feature A\n"
    );
    assert!(!repo.root.join(FEATURE_PLAN).exists());
    assert!(!repo.root.join("doc/plans/archive").exists());
    let moved = format!("Moved {FEATURE_PLAN} to {FEATURE_RETIRED}.");
    assert_eq!(lines.last(), Some(&moved));
    assert!(!repo.marker(&runtime).exists());
}

#[test]
#[serial]
fn finish_abandon_finds_the_marked_feature_home_even_with_the_same_slug_elsewhere() {
    let repo = Repo::new();
    repo.put(FEATURE_PLAN, "# Feature A\n");
    repo.put(PLAN, "# Standalone A\n");
    repo.put(
        "doc/plans/another-feature/IN_PROGRESS-PLAN-a.md",
        "# Other A\n",
    );
    repo.commit_all("plans");
    let runtime = repo.runtime();
    marker::write(&runtime, &feature_plan()).unwrap();

    assert_eq!(
        stranded_plan(&repo.root, &runtime).as_deref(),
        Some(FEATURE_PLAN)
    );
    let outcome = finish_abandon(&repo.root, &runtime).unwrap();

    assert_eq!(
        outcome,
        AbandonOutcome::Renamed {
            to: PathBuf::from(FEATURE_RETIRED)
        }
    );
    assert!(repo.has(FEATURE_RETIRED));
    assert!(!repo.root.join(FEATURE_PLAN).exists());
    assert!(repo.has(PLAN));
    assert!(repo.has("doc/plans/another-feature/IN_PROGRESS-PLAN-a.md"));
    assert!(!repo.root.join(RETIRED).exists());
    assert!(!repo.marker(&runtime).exists());
}

#[test]
#[serial]
fn a_symlinked_feature_archive_refuses_the_rename() {
    let repo = Repo::new();
    repo.put(FEATURE_PLAN, "# Feature A\n");
    let outside = tempfile::tempdir().unwrap();
    symlink(
        outside.path(),
        repo.root.join("doc/plans/my-feature/archive"),
    )
    .unwrap();

    let outcome = rename_to_retired(&repo.root, &feature_plan()).unwrap();

    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{outcome:?}"
    );
    assert!(repo.has(FEATURE_PLAN));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
#[serial]
fn a_symlinked_feature_home_refuses_retirement_and_recovery() {
    let repo = Repo::new();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("IN_PROGRESS-PLAN-a.md"),
        "# Feature A\n",
    )
    .unwrap();
    fs::create_dir_all(repo.root.join("doc/plans")).unwrap();
    symlink(outside.path(), repo.root.join("doc/plans/my-feature")).unwrap();
    let runtime = repo.runtime();
    marker::write(&runtime, &feature_plan()).unwrap();

    let outcome = rename_to_retired(&repo.root, &feature_plan()).unwrap();

    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{outcome:?}"
    );
    assert_eq!(stranded_plan(&repo.root, &runtime), None);
    assert_eq!(
        fs::read_to_string(outside.path().join("IN_PROGRESS-PLAN-a.md")).unwrap(),
        "# Feature A\n"
    );
    assert!(!outside.path().join("archive").exists());
}
