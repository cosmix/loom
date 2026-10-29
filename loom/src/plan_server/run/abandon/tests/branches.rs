//! The unmerged `loom/*` branch checks before and after `loom stop`.

use std::path::{Path, PathBuf};

use serial_test::serial;

use super::super::AbandonOutcome;
use super::{EnvGuard, Repo, CLEAN_REMOVES_WORK, PLAN, RETIRED};
use crate::commands::clean::unmerged_loom_branches;

/// Makes every later git command in the repository fail: git refuses a repository format it
/// does not know.
const BREAK_GIT: &str = "git -C \"$ROOT\" config core.repositoryformatversion 99\n";

fn branches(list: &[(&str, usize)]) -> Vec<(String, usize)> {
    list.iter().map(|(b, n)| (b.to_string(), *n)).collect()
}

fn assert_listing_refused(outcome: &AbandonOutcome) {
    match outcome {
        AbandonOutcome::Refused { reason } => {
            assert!(reason.contains("cannot be listed"), "{reason}")
        }
        other => panic!("expected Refused, got {other:?}"),
    }
}

#[test]
#[serial]
fn a_confirmed_list_that_differs_runs_nothing() {
    let repo = Repo::running();
    repo.branch_ahead("loom/s1", 2);
    let runtime = repo.runtime();
    let outcome = repo.abandon(&runtime, &branches(&[("loom/s1", 1)]));
    let now = branches(&[("loom/s1", 2)]);
    assert_eq!(outcome, AbandonOutcome::BranchesChanged { now });
    assert!(repo.calls().is_empty(), "{:?}", repo.calls());
    assert!(
        !repo.marker(&runtime).exists(),
        "the marker outlived BranchesChanged"
    );
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn a_list_that_grew_since_the_view_answers_branches_changed() {
    let repo = Repo::running();
    let shown = unmerged_loom_branches(&repo.root).branches;
    assert!(shown.is_empty(), "{shown:?}");
    repo.branch_ahead("loom/s1", 1);
    let outcome = repo.abandon(&repo.runtime(), &shown);
    let now = branches(&[("loom/s1", 1)]);
    assert_eq!(outcome, AbandonOutcome::BranchesChanged { now });
    assert!(repo.calls().is_empty(), "{:?}", repo.calls());
}

#[test]
#[serial]
fn the_matching_list_sets_clean_yes_for_clean_only_and_renames() {
    let repo = Repo::running();
    repo.branch_ahead("loom/s1", 1);
    repo.on(
        "clean",
        &format!("{CLEAN_REMOVES_WORK}git -C \"$ROOT\" branch -D -q loom/s1\n"),
    );
    let runtime = repo.runtime();
    let _inherited = EnvGuard::set("LOOM_CLEAN_YES", Path::new("1"));
    let outcome = repo.abandon(&runtime, &branches(&[("loom/s1", 1)]));
    let to = PathBuf::from(RETIRED);
    assert_eq!(outcome, AbandonOutcome::Renamed { to });
    let calls = vec!["stop LOOM_CLEAN_YES=unset", "clean LOOM_CLEAN_YES=1"];
    assert_eq!(repo.calls(), calls);
}

#[test]
#[serial]
fn a_stop_that_commits_on_a_loom_branch_ends_before_clean() {
    let repo = Repo::running();
    let commit = "c=$(git -C \"$ROOT\" commit-tree -p HEAD -m stage 'HEAD^{tree}') || exit 1\n\
                  git -C \"$ROOT\" update-ref refs/heads/loom/s2 \"$c\"\n";
    repo.on("stop", commit);
    let runtime = repo.runtime();
    let outcome = repo.abandon(&runtime, &[]);
    let now = branches(&[("loom/s2", 1)]);
    assert_eq!(outcome, AbandonOutcome::BranchesChanged { now });
    assert_eq!(repo.calls(), vec!["stop LOOM_CLEAN_YES=unset"]);
    assert!(
        !repo.marker(&runtime).exists(),
        "the marker outlived BranchesChanged"
    );
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn a_branch_listing_error_is_reported_not_an_empty_list() {
    let repo = Repo::running();
    repo.git(&["config", "core.repositoryformatversion", "99"]);
    let listing = unmerged_loom_branches(&repo.root);
    assert!(listing.branches.is_empty(), "{:?}", listing.branches);
    assert!(listing.listing_error.is_some());
}

#[test]
#[serial]
fn a_branch_listing_error_before_stop_runs_nothing() {
    let repo = Repo::running();
    repo.git(&["config", "core.repositoryformatversion", "99"]);
    let runtime = repo.runtime();
    assert_listing_refused(&repo.abandon(&runtime, &[]));
    assert!(repo.calls().is_empty(), "{:?}", repo.calls());
    assert!(!repo.marker(&runtime).exists(), "the marker was written");
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn a_branch_listing_error_after_stop_ends_before_clean() {
    let repo = Repo::running();
    repo.on("stop", BREAK_GIT);
    let runtime = repo.runtime();
    let (outcome, lines) = repo.abandon_lines(&runtime);
    assert_listing_refused(&outcome);
    assert_eq!(repo.calls(), vec!["stop LOOM_CLEAN_YES=unset"]);
    assert!(
        lines
            .last()
            .is_some_and(|line| line.contains("cannot be listed")),
        "{lines:?}"
    );
    assert!(
        !repo.marker(&runtime).exists(),
        "nothing was deleted: the marker must go"
    );
    assert!(repo.has(PLAN));
    assert!(repo.root.join(".loom/work").exists());
}

#[test]
#[serial]
fn a_branch_whose_ahead_count_fails_counts_as_unmerged() {
    let repo = Repo::running();
    // The workspace names a target branch that does not exist.
    repo.workspace(PLAN, "no-such-branch");
    repo.git(&["branch", "loom/s1"]);
    let listing = unmerged_loom_branches(&repo.root);
    assert_eq!(listing.target_branch, "no-such-branch");
    assert_eq!(listing.branches, vec![("loom/s1".to_string(), 1)]);
}
