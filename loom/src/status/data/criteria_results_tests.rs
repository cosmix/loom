use super::*;
use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase, CriterionResult,
    VerificationCheckpoint,
};
use crate::handoff::HandoffV2;
use crate::models::stage::AcceptanceCriterion;
use tempfile::TempDir;

fn work_dir() -> (TempDir, WorkDir) {
    let temp = TempDir::new().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    (temp, work)
}

fn blocked_stage() -> Stage {
    let mut stage = Stage::new("server".to_string(), None);
    stage.id = "server".to_string();
    stage.status = StageStatus::Blocked;
    stage.session = Some("session-1".to_string());
    stage.acceptance = (0..3)
        .map(|index| AcceptanceCriterion::Simple(format!("check {index}")))
        .collect();
    stage
}

fn record(work: &WorkDir, results: &[(&str, bool)], diagnostic: Option<&str>) {
    let evidence = CompletionAttemptEvidence {
        version: 1,
        stage_id: "server".into(),
        session_id: "session-1".into(),
        commit: "a".repeat(40),
        check_definition_hash: "b".repeat(64),
        exact_command: "loom stage complete server".into(),
        evidence_nonce: "nonce-000000000000000001".into(),
        verification: VerificationCheckpoint {
            criteria: results
                .iter()
                .map(|(id, passed)| CriterionResult {
                    id: (*id).into(),
                    passed: *passed,
                })
                .collect(),
            environment_policy: "trusted-host".into(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::ToolFailed,
        external_failure_code: None,
        diagnostic_first_line: diagnostic.map(str::to_owned),
        observed_at: "2026-09-14T10:00:00Z".into(),
        attestation: None,
    };
    let mut checkpoint = CompletionCheckpoint::new("server", "session-1");
    checkpoint.record_attempt(&evidence).unwrap();
    let handoff =
        HandoffV2::new("session-1", "server").with_completion_checkpoint(Some(checkpoint));
    let dir = work.root().join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("server-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();
}

#[test]
fn a_failed_criterion_maps_to_its_acceptance_index_and_command() {
    let (_temp, work) = work_dir();
    record(
        &work,
        &[("acceptance-0", true), ("acceptance-2", false)],
        Some("FAILED [criterion 2]"),
    );

    let failing = failing_criteria(&blocked_stage(), &[], &work);

    assert_eq!(
        failing,
        [FailingCriterion {
            index: 2,
            command: "check 2".into(),
            diagnostic: Some("FAILED [criterion 2]".into()),
        }]
    );
}

#[test]
fn several_failures_carry_no_diagnostic_and_unmappable_ids_are_dropped() {
    let (_temp, work) = work_dir();
    record(
        &work,
        &[
            ("acceptance-0", false),
            ("acceptance-1", false),
            ("acceptance-9", false),
            ("goal-backward", false),
        ],
        Some("one line"),
    );

    let failing = failing_criteria(&blocked_stage(), &[], &work);

    assert_eq!(failing.iter().map(|f| f.index).collect::<Vec<_>>(), [0, 1]);
    assert!(failing.iter().all(|f| f.diagnostic.is_none()));
}

#[test]
fn no_checkpoint_or_another_status_yields_nothing() {
    let (_temp, work) = work_dir();
    assert!(failing_criteria(&blocked_stage(), &[], &work).is_empty());

    record(&work, &[("acceptance-1", false)], None);
    let mut running = blocked_stage();
    running.status = StageStatus::Executing;
    assert!(failing_criteria(&running, &[], &work).is_empty());
}
