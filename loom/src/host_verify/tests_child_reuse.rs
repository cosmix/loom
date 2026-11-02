//! Tests for which criteria a host child runs: a check's selection, and the
//! cached passes a completion reuses from a check on the same tree. Each
//! starts loom's real confinement, so each is serial and skips where it
//! cannot start.

use serial_test::serial;

use super::store;
use super::tests_child::{confinement_unavailable, git, labels_and_verdicts, Fixture, STAGE};
use super::{JobKind, JobRecord, JobResult, JobState};
use crate::fs::session_files::save_session;
use crate::models::session::Session;
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

/// Make the fixture's job session a merge resolver.
fn resolver_session(fx: &Fixture, id: &str) {
    let mut session = Session::new_merge(format!("loom/{STAGE}"), "main".to_string());
    session.id = id.to_string();
    save_session(&session, &fx.work_dir).unwrap();
}

/// After a handback the stage session's Complete job must run every criterion:
/// `land_stage_merge` runs no acceptance gate of its own.
#[test]
#[serial]
fn a_resolvers_check_stores_no_pass_a_complete_could_reuse() {
    if confinement_unavailable("a_resolvers_check_stores_no_pass_a_complete_could_reuse") {
        return;
    }
    let fx = Fixture::new(&["test -f .gitignore"], StageType::Standard);
    let check = fx.job(JobState::Running, false);
    resolver_session(&fx, &check.session_id);
    assert!(fx.run(&check).passed);
    let completion = complete(&fx);

    let result = fx.run(&completion);

    let acceptance = result
        .criteria
        .iter()
        .find(|outcome| outcome.label == "acceptance-0")
        .expect("the criterion's outcome");
    assert!(
        !acceptance.summary.contains("(cached)"),
        "{}",
        acceptance.summary
    );
    assert!(acceptance.log.is_some(), "the criterion ran");
}

#[test]
#[serial]
fn a_resolvers_check_reads_no_pass_another_session_stored() {
    if confinement_unavailable("a_resolvers_check_reads_no_pass_another_session_stored") {
        return;
    }
    let fx = Fixture::new(&["test -f .gitignore"], StageType::Standard);
    let stage_check = fx.job(JobState::Running, false);
    assert!(fx.run(&stage_check).passed);
    let cached = fx.run(&fx.job(JobState::Running, false));
    assert!(
        cached.criteria[0].summary.contains("(cached)"),
        "the fixture never reaches the pass cache: {cached:?}"
    );
    let check = fx.job(JobState::Running, false);
    resolver_session(&fx, &check.session_id);

    let result = fx.run(&check);

    assert!(result.passed, "{result:?}");
    assert!(
        !result.criteria[0].summary.contains("(cached)"),
        "{result:?}"
    );
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
