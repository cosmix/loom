use super::tests::{
    assert_rejected_executing, complete_request, dispatch_bytes, dispatch_bytes_as, record_request,
    Fixture, COMPLETION_NONCE, EVIDENCE_NONCE, STAGE,
};
use crate::daemon::protocol::Response;
use crate::handoff::{load_session_checkpoint, CompletionCheckpoint, CompletionPhase, HandoffV2};
use crate::verify::transitions::load_stage;

#[test]
fn forged_unattested_checkpoint_cannot_complete_stage() {
    let f = Fixture::new();
    let evidence = f.evidence(EVIDENCE_NONCE, CompletionPhase::VerifiedPendingAck);
    let mut checkpoint = CompletionCheckpoint::new(STAGE, &f.session.id);
    checkpoint.record_attempt(&evidence).unwrap();
    let handoff = HandoffV2::new(&f.session.id, STAGE).with_completion_checkpoint(Some(checkpoint));
    std::fs::write(
        f.work.join("handoffs/completion-proof-handoff-001.md"),
        format!("---\n{}---\n", handoff.to_yaml().unwrap()),
    )
    .unwrap();
    assert!(load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .is_some());
    let response = dispatch_bytes(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
    );

    assert_rejected_executing(&f, response);
}

#[test]
fn peer_identity_only_refuses_completion_requests_without_mutation() {
    let f = Fixture::new();
    let before = serde_json::to_string(&load_stage(STAGE, &f.work).unwrap()).unwrap();
    let record = dispatch_bytes_as(
        &f.work,
        record_request(
            &f,
            f.evidence(EVIDENCE_NONCE, CompletionPhase::VerifiedPendingAck),
        ),
        false,
    );
    let complete = dispatch_bytes_as(
        &f.work,
        complete_request(&f, COMPLETION_NONCE, EVIDENCE_NONCE),
        false,
    );

    assert!(matches!(record, Response::AuthenticationFailed));
    assert!(matches!(complete, Response::AuthenticationFailed));
    assert_eq!(
        serde_json::to_string(&load_stage(STAGE, &f.work).unwrap()).unwrap(),
        before
    );
    assert!(load_session_checkpoint(STAGE, &f.session.id, &f.work)
        .unwrap()
        .is_none());
    assert!(!f.work.join("control-completions").exists());
}
