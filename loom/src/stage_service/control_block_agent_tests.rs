//! An agent's block after a failed `loom stage complete` is refused; the
//! operator's block is not.

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::handoff::record_attempt_handoff;
use crate::handoff::schema::{
    CompletionAttemptEvidence, CriterionResult, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::ContractSpec;
use crate::verify::transitions::save_stage;

const STAGE: &str = "stage-blk";

#[test]
fn a_prechecked_old_session_cannot_block_the_new_attempt() {
    let (temp, session, _) = fixture();
    crate::verify::transitions::update_stage(STAGE, temp.path(), |stage| {
        stage.session = Some("replacement-session".into());
        Ok(())
    })
    .unwrap();
    let response =
        handle_agent_block_for_session(temp.path(), STAGE, "stale", Some(&session.id)).unwrap();
    assert!(matches!(response, Response::Error { .. }));
    let current = crate::verify::transitions::load_stage(STAGE, temp.path()).unwrap();
    assert_eq!(current.status, StageStatus::Executing);
    assert_eq!(current.session.as_deref(), Some("replacement-session"));
}

/// An executing stage whose current session is `session-blk`.
fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("handoffs")).unwrap();
    let mut session = Session::new();
    session.id = "session-blk".to_string();
    let mut stage = Stage::new("blk".to_string(), None);
    stage.id = STAGE.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    save_stage(&stage, temp.path()).unwrap();
    (temp, session, stage)
}

/// Record a signed completion attempt in `phase` for the stage's session.
fn record(temp: &TempDir, session: &Session, stage: &Stage, phase: CompletionPhase) {
    record_with(temp, session, stage, phase, &[("acceptance-2", false)]);
}

/// As [`record`], with the attempt's per-criterion results.
fn record_with(
    temp: &TempDir,
    session: &Session,
    stage: &Stage,
    phase: CompletionPhase,
    results: &[(&str, bool)],
) {
    let evidence = CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: "a".repeat(40),
        check_definition_hash: "b".repeat(64),
        exact_command: "loom stage complete stage-blk".to_string(),
        evidence_nonce: "evidence_nonce_000000001".to_string(),
        verification: VerificationCheckpoint {
            criteria: results
                .iter()
                .map(|(id, passed)| CriterionResult {
                    id: id.to_string(),
                    passed: *passed,
                })
                .collect(),
            environment_policy: "stage-host-allowlist-v2".to_string(),
            environment: Vec::new(),
        },
        phase,
        external_failure_code: None,
        diagnostic_first_line: Some("criterion 13 failed".to_string()),
        observed_at: "2026-10-20T10:00:00Z".to_string(),
        attestation: None,
    };
    record_attempt_handoff(session, stage, &evidence, temp.path()).unwrap();
}

#[test]
fn an_agent_block_after_a_failed_completion_is_refused_with_the_dispute_route() {
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, CompletionPhase::ToolFailed);

    let response = handle_agent_block_stage(temp.path(), STAGE, "criterion 14 cannot run").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    for route in [
        "loom stage dispute-criteria stage-blk --criterion-index 2 --reason",
        "loom stage dispute-findings",
        "loom stage dispute-integrity",
        "--field wiring",
        "operator automatically",
    ] {
        assert!(message.contains(route), "missing {route:?}: {message}");
    }
    for other_kind in [
        "--field impact-tests",
        "dispute-contract stage-blk",
        "contracts",
    ] {
        assert!(!message.contains(other_kind), "{other_kind:?}: {message}");
    }
    let after = load_stage(STAGE, temp.path()).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert!(after.close_reason.is_none());
}

#[test]
fn an_agent_block_without_a_failed_completion_blocks() {
    let (temp, _, _) = fixture();

    let response = handle_agent_block_stage(temp.path(), STAGE, "spec is ambiguous").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
    let after = load_stage(STAGE, temp.path()).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    assert_eq!(after.close_reason.as_deref(), Some("spec is ambiguous"));
}

#[test]
fn a_verified_or_unsigned_completion_does_not_refuse_the_block() {
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, CompletionPhase::VerifiedPendingAck);
    let response = handle_agent_block_stage(temp.path(), STAGE, "daemon gone").unwrap();
    assert!(matches!(response, Response::Ok), "{response:?}");

    // A failure signed with a key the daemon no longer holds is not trusted.
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, CompletionPhase::ToolFailed);
    fs::remove_file(
        temp.path()
            .join(crate::handoff::completion::ATTESTATION_KEY_FILE),
    )
    .unwrap();
    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();
    assert!(matches!(response, Response::Ok), "{response:?}");
}

#[test]
fn an_operator_block_after_a_failed_completion_still_blocks() {
    let (temp, session, stage) = fixture();
    record(&temp, &session, &stage, CompletionPhase::ToolFailed);

    let response = handle_block_stage(temp.path(), STAGE, "operator stop").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
    assert_eq!(
        load_stage(STAGE, temp.path()).unwrap().status,
        StageStatus::Blocked
    );
}

#[test]
fn a_refusal_names_every_failing_acceptance_index() {
    let (temp, session, stage) = fixture();
    let results = [
        ("acceptance-7", false),
        ("acceptance-3", false),
        ("acceptance-5", true),
        ("wiring-0", false),
    ];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(
        message.contains("--criterion-index 3 --criterion-index 7 --reason"),
        "{message}"
    );
    assert!(!message.contains("--criterion-index 5"), "{message}");
}

#[test]
fn a_failed_completion_with_no_failing_criterion_still_blocks() {
    let (temp, session, stage) = fixture();
    record_with(&temp, &session, &stage, CompletionPhase::ToolFailed, &[]);

    let response = handle_agent_block_stage(temp.path(), STAGE, "bwrap missing").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
    let after = load_stage(STAGE, temp.path()).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
}

fn contract(id: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: format!("tests/{id}.rs"),
        test: None,
        runner: None,
        cwd: None,
        scenario: "a scenario".to_string(),
        rejects: "a wrong implementation".to_string(),
    }
}

/// The stage of [`fixture`] with frozen contracts `ids`.
fn fixture_with_contracts(ids: &[&str]) -> (TempDir, Session, Stage) {
    let (temp, session, mut stage) = fixture();
    stage.contracts = ids.iter().map(|id| contract(id)).collect();
    let contracts = stage.contracts.clone();
    crate::verify::transitions::update_stage(STAGE, temp.path(), |saved| {
        saved.contracts = contracts;
        Ok(())
    })
    .unwrap();
    (temp, session, stage)
}

#[test]
fn a_failing_frozen_contract_refuses_the_block_and_names_the_contracts() {
    let (temp, session, stage) = fixture_with_contracts(&["web-wheel", "rust-history"]);
    let results = [("acceptance-1", true), ("contracts", false)];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "contracts cannot pass").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(
        message.contains(
            "dispute-contract stage-blk --contract web-wheel --contract rust-history --reason"
        ),
        "{message}"
    );
    for route in [
        "loom check stage-blk --contracts",
        "loom stage contracts restore stage-blk",
        "drop the flags of those that pass",
        "operator automatically",
    ] {
        assert!(message.contains(route), "missing {route:?}: {message}");
    }
    for other_kind in ["--criterion-index", "--field impact-tests"] {
        assert!(!message.contains(other_kind), "{other_kind:?}: {message}");
    }
    let after = load_stage(STAGE, temp.path()).unwrap();
    assert_eq!(after.status, StageStatus::Executing);
    assert!(after.close_reason.is_none());
}

#[test]
fn a_failing_impact_test_run_refuses_the_block() {
    let (temp, session, stage) = fixture();
    let results = [("acceptance-1", true), ("impact-tests", false)];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(
        message.contains("dispute-criteria stage-blk --field impact-tests --test <id> --reason"),
        "{message}"
    );
    for other_kind in [
        "--criterion-index",
        "dispute-contract stage-blk",
        "contracts restore",
    ] {
        assert!(!message.contains(other_kind), "{other_kind:?}: {message}");
    }
}

#[test]
fn every_failed_kind_gets_its_own_route() {
    let (temp, session, stage) = fixture_with_contracts(&["web-wheel"]);
    let results = [
        ("acceptance-4", false),
        ("contracts", false),
        ("impact-tests", false),
    ];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    for route in [
        "dispute-criteria stage-blk --criterion-index 4 --reason",
        "--field impact-tests --test <id>",
        "dispute-contract stage-blk --contract web-wheel --reason",
        "contracts restore stage-blk",
    ] {
        assert!(message.contains(route), "missing {route:?}: {message}");
    }
}

#[test]
fn a_passing_contract_check_and_other_failed_gates_still_allow_the_block() {
    let (temp, session, stage) = fixture_with_contracts(&["web-wheel"]);
    let results = [
        ("acceptance-1", true),
        ("contracts", true),
        ("unwired", false),
    ];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
}

#[test]
fn an_indexed_goal_gap_refuses_the_block_and_names_its_dispute_route() {
    let (temp, session, stage) = fixture();
    let results = [
        ("goal-backward", false),
        ("artifact-1", false),
        ("wiring-0", false),
        ("wiring_tests-2", false),
    ];
    let phase = CompletionPhase::ToolFailed;
    record_with(&temp, &session, &stage, phase, &results);

    let response = handle_agent_block_stage(temp.path(), STAGE, "x").unwrap();

    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    for route in [
        "dispute-criteria stage-blk --field artifacts --criterion-index 1 --reason",
        "dispute-criteria stage-blk --field wiring --criterion-index 0 --reason",
        "dispute-criteria stage-blk --field wiring-tests --criterion-index 2 --reason",
    ] {
        assert!(message.contains(route), "missing {route:?}: {message}");
    }
    assert_eq!(
        load_stage(STAGE, temp.path()).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn a_failed_goal_backward_without_an_indexed_gap_still_blocks() {
    let (temp, session, stage) = fixture();
    let results = [("goal-backward", false)];
    record_with(
        &temp,
        &session,
        &stage,
        CompletionPhase::ToolFailed,
        &results,
    );

    let response = handle_agent_block_stage(temp.path(), STAGE, "dead code").unwrap();

    assert!(matches!(response, Response::Ok), "{response:?}");
}
