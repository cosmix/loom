//! Tests for which criteria a host child runs: a check's selection, and the
//! cached passes a completion reuses from a check on the same tree. Each
//! starts loom's real confinement, so each is serial and skips where it
//! cannot start.

use serial_test::serial;

use super::store;
use super::tests_child::{confinement_unavailable, git, labels_and_verdicts, Fixture, STAGE};
use super::{JobKind, JobRecord, JobResult, JobState};
use crate::models::stage::StageType;

#[test]
#[serial]
fn a_check_with_a_selection_runs_only_its_criteria() {
    if confinement_unavailable("a_check_with_a_selection_runs_only_its_criteria") {
        return;
    }
    let fx = Fixture::new(&["false", "true", "false"], StageType::Standard);
    let mut record = fx.job(JobState::Running, false);
    record.criteria = Some(vec![1]);
    store::save_current(&fx.work_dir, &record).unwrap();

    let result = fx.run(&record);

    assert!(result.passed, "{result:?}");
    assert_eq!(labels_and_verdicts(&result), [("acceptance-1", true)]);
}

#[test]
#[serial]
fn a_complete_after_a_passing_check_reuses_every_cached_pass() {
    if confinement_unavailable("a_complete_after_a_passing_check_reuses_every_cached_pass") {
        return;
    }
    let fx = Fixture::new(
        &["test -f .gitignore", "test -s .gitignore"],
        StageType::Standard,
    );
    assert!(fx.run(&fx.job(JobState::Running, false)).passed);
    let completion = complete(&fx);

    assert_every_acceptance_cached(&fx, &completion, &fx.run(&completion));
}

/// A completion runs on a commit whose content a check passed uncommitted:
/// the pass cache keys on content, never on the commit or the index.
#[test]
#[serial]
fn a_complete_on_the_commit_of_a_checked_dirty_tree_reuses_every_cached_pass() {
    if confinement_unavailable(
        "a_complete_on_the_commit_of_a_checked_dirty_tree_reuses_every_cached_pass",
    ) {
        return;
    }
    let fx = Fixture::new(
        &["test -s notes.txt", "grep -q edited notes.txt"],
        StageType::Standard,
    );
    std::fs::write(fx.worktree.join("notes.txt"), "edited\n").unwrap();
    assert!(fx.run(&fx.job(JobState::Running, false)).passed);
    git(&fx.worktree, &["add", "notes.txt"]);
    git(&fx.worktree, &["commit", "-q", "-m", "checked content"]);
    let completion = complete(&fx);

    assert_every_acceptance_cached(&fx, &completion, &fx.run(&completion));
}

fn complete(fx: &Fixture) -> JobRecord {
    let mut completion = fx.job(JobState::Running, false);
    completion.kind = JobKind::Complete;
    store::save_current(&fx.work_dir, &completion).unwrap();
    completion
}

fn assert_every_acceptance_cached(fx: &Fixture, completion: &JobRecord, result: &JobResult) {
    let acceptance: Vec<_> = result
        .criteria
        .iter()
        .filter(|outcome| outcome.label.starts_with("acceptance-"))
        .collect();
    assert_eq!(acceptance.len(), 2, "{result:?}");
    for outcome in acceptance {
        assert!(outcome.summary.contains("(cached)"), "{}", outcome.summary);
        assert_eq!(outcome.log, None, "a cached answer has no output");
    }
    let groups = store::load_groups(&fx.work_dir, STAGE, &completion.id).unwrap();
    assert!(
        groups.is_empty(),
        "the completion spawned criteria: {groups:?}"
    );
}
