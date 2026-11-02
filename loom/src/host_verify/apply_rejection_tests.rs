//! `receipt_recorded` decides whether a completed stage still needs its receipt,
//! so an unsigned artifact can neither stand in for it nor hide it.

use std::fs;

use super::super::apply::Applied;
use super::super::tests_complete_support::{Fixture, SESSION, STAGE};
use super::receipt_recorded;
use crate::handoff::{
    AcceptedReceipt, CompletionCheckpoint, CompletionPhase, HandoffV2, NonceObservation,
    MAX_EVIDENCE_NONCES,
};
use crate::models::stage::StageType;

fn unsigned_observation(nonce: String) -> NonceObservation {
    NonceObservation {
        evidence_nonce: nonce,
        identity_digest: "a".repeat(64),
        fingerprint: None,
        phase: CompletionPhase::ToolFailed,
        first_observed_at: "2026-09-14T10:00:00Z".to_string(),
        last_observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    }
}

fn write_unsigned(fx: &Fixture, checkpoint: CompletionCheckpoint) {
    let dir = fx.work_dir.join("handoffs");
    fs::create_dir_all(&dir).unwrap();
    let handoff = HandoffV2::new(SESSION, STAGE).with_completion_checkpoint(Some(checkpoint));
    fs::write(
        dir.join(format!("{STAGE}-handoff-001.md")),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();
}

#[test]
fn an_unsigned_receipt_does_not_stand_in_for_the_recorded_one() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    let mut forged = CompletionCheckpoint::new(STAGE, SESSION);
    forged
        .observations
        .push(unsigned_observation(record.id.clone()));
    forged
        .record_accepted(AcceptedReceipt {
            evidence_nonce: record.id.clone(),
            completion_nonce: "completion_nonce_forged01".to_string(),
            commit: record.head.clone(),
            attestation: None,
        })
        .unwrap();
    write_unsigned(&fx, forged);
    assert!(!receipt_recorded(&fx.work_dir, &record).unwrap());

    assert!(matches!(fx.apply(&record), Ok(Applied::Completed { .. })));

    assert!(receipt_recorded(&fx.work_dir, &record).unwrap());
}

#[test]
fn unsigned_nonce_fillers_do_not_hide_the_recorded_receipt() {
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    let mut fillers = CompletionCheckpoint::new(STAGE, SESSION);
    for index in 0..MAX_EVIDENCE_NONCES {
        fillers
            .observations
            .push(unsigned_observation(format!("evidence_forged_{index:04}")));
    }
    write_unsigned(&fx, fillers);

    assert!(matches!(fx.apply(&record), Ok(Applied::Completed { .. })));

    assert!(receipt_recorded(&fx.work_dir, &record).unwrap());
}
