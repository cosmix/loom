//! Tests for which criteria a host child runs: a check's selection, and the
//! cached passes a completion reuses from a check on the same tree. Each
//! starts loom's real confinement, so each is serial and skips where it
//! cannot start.

use serial_test::serial;

use super::store;
use super::tests_child::{confinement_unavailable, labels_and_verdicts, Fixture, STAGE};
use super::{JobKind, JobState};
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
    let mut completion = fx.job(JobState::Running, false);
    completion.kind = JobKind::Complete;
    store::save_current(&fx.work_dir, &completion).unwrap();

    let result = fx.run(&completion);

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
