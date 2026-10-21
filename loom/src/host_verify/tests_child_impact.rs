//! What the host child's completion run keeps of the impact gate's result
//! (`child_complete::observe_impact`, `record_failed_impact`): a failing run
//! replaces `reviews/<stage>/impact-observed.json`, a passing one leaves it.

use anyhow::anyhow;
use tempfile::TempDir;

use super::child_complete::{observe_impact, record_failed_impact};
use crate::commands::stage::complete::complete_verification::IMPACT_TESTS_LABEL;
use crate::verify::impact_tests::{
    load_observed, record_observed, FailureKind, ImpactFailure, ImpactTestsFailed, ObservedImpact,
};

const STAGE: &str = "stage-a";

fn failures() -> Vec<ImpactFailure> {
    vec![ImpactFailure {
        id: "IT-cargo-test:.".to_string(),
        command: "cargo test -- a".to_string(),
        tests: vec!["a".to_string()],
        kind: FailureKind::Fails { failed: Some(2) },
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output: "a failed".to_string(),
    }]
}

fn failed_selection() -> ObservedImpact {
    ObservedImpact::of(&failures())
}

#[test]
fn a_failed_run_writes_the_failures_its_impact_selection_observed() {
    let work = TempDir::new().unwrap();
    let observed = failed_selection();

    record_failed_impact(work.path(), STAGE, Some(observed.clone()), false);

    assert_eq!(load_observed(work.path(), STAGE).unwrap(), observed);
}

#[test]
fn an_impact_gate_failure_is_observed_as_the_failures_it_carries() {
    let mut impact = Some(ObservedImpact::default());

    let gate = Err(anyhow::Error::new(ImpactTestsFailed::for_test(
        "s",
        failures(),
    )));
    observe_impact(&mut impact, IMPACT_TESTS_LABEL, &gate);

    assert_eq!(impact, Some(failed_selection()));
}

#[test]
fn a_run_that_failed_for_another_reason_clears_stale_failures() {
    let work = TempDir::new().unwrap();
    record_observed(work.path(), STAGE, &failed_selection()).unwrap();
    let mut impact = Some(failed_selection());

    let other = Err(anyhow!("the formatting check failed"));
    observe_impact(&mut impact, IMPACT_TESTS_LABEL, &other);
    assert_eq!(impact, Some(ObservedImpact::default()));
    record_failed_impact(work.path(), STAGE, impact, false);

    assert_eq!(
        load_observed(work.path(), STAGE).unwrap(),
        ObservedImpact::default()
    );
}

#[test]
fn a_passing_run_writes_nothing() {
    let work = TempDir::new().unwrap();

    record_failed_impact(work.path(), STAGE, Some(failed_selection()), true);

    assert!(!work.path().join("reviews").join(STAGE).exists());
    assert_eq!(
        load_observed(work.path(), STAGE).unwrap(),
        ObservedImpact::default()
    );
}

#[test]
fn a_stage_without_a_record_writes_nothing_even_when_it_fails() {
    let work = TempDir::new().unwrap();

    record_failed_impact(work.path(), STAGE, None, false);

    assert!(!work.path().join("reviews").exists());
}

#[test]
fn only_the_impact_check_updates_what_was_observed() {
    let mut impact = Some(failed_selection());
    observe_impact(&mut impact, "acceptance", &Err(anyhow!("nope")));
    assert_eq!(impact, Some(failed_selection()));

    let mut none = None;
    observe_impact(&mut none, IMPACT_TESTS_LABEL, &Ok(()));
    assert_eq!(none, None);

    observe_impact(&mut impact, IMPACT_TESTS_LABEL, &Ok(()));
    assert_eq!(impact, Some(ObservedImpact::default()));
}
