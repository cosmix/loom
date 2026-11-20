//! Contracts for `check_narrowing`: a criterion verdict may leave tests out of
//! a stage's check only on a recorded base run that fails them, and every
//! test it leaves out becomes a hand-off record.

use serde_json::{json, Value};

use super::verdict_narrowing::check_narrowing;
use crate::verify::review::handoffs::{BaseEvidence, Handoff};

const STAGE: &str = "stage-under-dispute";
const DISPUTE: &str = "dispute-0001";
const FULL_SUITE: &str = "cargo test";
const E2E_SUITE: &str = "cargo test --test e2e";

fn current() -> Vec<Value> {
    vec![json!(FULL_SUITE), json!({ "command": E2E_SUITE })]
}

fn leaves_out(index: usize, tests: &[&str]) -> Value {
    json!({
        "index": index,
        "tests": tests,
        "runs_at": "integration-verify",
        "reason": "these tests need outbound network",
    })
}

fn replace_narrowing() -> Value {
    json!({
        "field": "acceptance",
        "patches": [{
            "op": "replace",
            "index": 0,
            "value": "cargo test -- --skip net_fetch --skip net_upload",
        }],
        "leaves_out": [leaves_out(0, &["net_fetch", "net_upload"])],
    })
}

fn evidence(index: usize, command: &str) -> BaseEvidence {
    BaseEvidence {
        index,
        command: command.to_string(),
        passed: false,
        executed: Some(40),
        failed: Some(2),
        build_failed: false,
        timed_out: false,
    }
}

fn refusal(plan_patch: &Value, evidence: &[BaseEvidence]) -> String {
    match check_narrowing(STAGE, DISPUTE, plan_patch, &current(), evidence) {
        Ok(handoffs) => panic!("the narrowing must be refused, got {handoffs:?}"),
        Err(text) => text,
    }
}

fn assert_refused(plan_patch: &Value, evidence: &[BaseEvidence], needle: &str) {
    let text = refusal(plan_patch, evidence);
    assert!(
        text.contains(needle),
        "refusal must contain {needle:?}, got: {text}"
    );
}

fn handoff(index: usize, tests: &[&str], verified: bool) -> Handoff {
    Handoff {
        stage: STAGE.to_string(),
        field: "acceptance".to_string(),
        index,
        tests: tests.iter().map(|test| test.to_string()).collect(),
        runs_at: "integration-verify".to_string(),
        reason: "these tests need outbound network".to_string(),
        dispute: DISPUTE.to_string(),
        verified,
    }
}

#[test]
fn a_narrowing_needs_a_failing_base_run() {
    let patch = replace_narrowing();

    // The judge's statement alone: no base run recorded.
    assert_refused(&patch, &[], "--at-base");

    // A base run of another command does not apply to the entry.
    assert_refused(&patch, &[evidence(0, "cargo test --lib")], "--at-base");

    // A base run of the entry at another index does not apply either.
    assert_refused(&patch, &[evidence(1, FULL_SUITE)], "--at-base");

    let passing = BaseEvidence {
        passed: true,
        failed: Some(0),
        ..evidence(0, FULL_SUITE)
    };
    assert_refused(&patch, &[passing], "passes at the base");

    let failing = evidence(0, FULL_SUITE);
    let handoffs = check_narrowing(STAGE, DISPUTE, &patch, &current(), &[failing])
        .expect("a narrowing on a base run that fails 2 of 40 tests is accepted");
    assert_eq!(
        handoffs,
        vec![handoff(0, &["net_fetch", "net_upload"], true)]
    );
}

#[test]
fn a_test_absent_at_base_is_never_narrowed() {
    let patch = replace_narrowing();

    let selected_none = BaseEvidence {
        executed: Some(0),
        failed: None,
        ..evidence(0, FULL_SUITE)
    };
    assert_refused(&patch, &[selected_none], "the stage's own");

    let build_failed = BaseEvidence {
        build_failed: true,
        executed: None,
        failed: None,
        ..evidence(0, FULL_SUITE)
    };
    assert_refused(&patch, &[build_failed], "the stage's own");
}

#[test]
fn a_deleted_criterion_is_handed_off() {
    let bare_delete = json!({
        "field": "acceptance",
        "patches": [{ "op": "delete", "index": 1 }],
    });
    assert_refused(&bare_delete, &[evidence(1, E2E_SUITE)], "leaves_out");

    let recorded_delete = json!({
        "field": "acceptance",
        "patches": [{ "op": "delete", "index": 1 }],
        "leaves_out": [leaves_out(1, &["e2e_full_flow"])],
    });
    let timed_out = BaseEvidence {
        timed_out: true,
        executed: None,
        failed: None,
        ..evidence(1, E2E_SUITE)
    };
    let handoffs = check_narrowing(STAGE, DISPUTE, &recorded_delete, &current(), &[timed_out])
        .expect("a delete that records what it leaves out, on a failing base run, is accepted");
    assert_eq!(handoffs, vec![handoff(1, &["e2e_full_flow"], false)]);
}

#[test]
fn an_amendment_equal_to_the_current_criterion_is_refused() {
    let same_string = json!({
        "field": "acceptance",
        "patches": [{ "op": "replace", "index": 0, "value": FULL_SUITE }],
    });
    assert_refused(&same_string, &[], "changes nothing");

    let same_object = json!({
        "field": "acceptance",
        "patches": [{ "op": "replace", "index": 1, "value": { "command": E2E_SUITE } }],
    });
    assert_refused(&same_object, &[], "changes nothing");

    // A replace that changes the criterion and leaves nothing out is no narrowing.
    let changed = json!({
        "field": "acceptance",
        "patches": [{ "op": "replace", "index": 0, "value": "cargo test --workspace" }],
    });
    let handoffs = check_narrowing(STAGE, DISPUTE, &changed, &current(), &[])
        .expect("a replace that changes the criterion is not refused");
    assert!(handoffs.is_empty(), "nothing was left out: {handoffs:?}");
}
