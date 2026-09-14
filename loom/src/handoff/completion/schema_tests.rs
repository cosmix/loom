use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint, CompletionPhase,
    CriterionResult, NonceObservation, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
    MAX_COMMAND_LEN, MAX_EVIDENCE_NONCES,
};

fn evidence() -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: "stage-1".into(),
        session_id: "session-1".into(),
        commit: "abcdef1".into(),
        check_definition_hash: "checks:v1".into(),
        exact_command: "cargo test --lib".into(),
        evidence_nonce: "evidence_nonce_01".into(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "tests".into(),
                passed: true,
            }],
            environment_policy: "linux-x86_64".into(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("ack_lost".into()),
        diagnostic_first_line: Some("daemon response was lost".into()),
        observed_at: "2026-09-14T10:00:00Z".into(),
        attestation: None,
    }
}

#[test]
fn evidence_serde_round_trip_preserves_current_schema() {
    let expected = evidence();
    let json = serde_json::to_value(&expected).unwrap();
    let actual: CompletionAttemptEvidence = serde_json::from_value(json.clone()).unwrap();

    assert_eq!(actual, expected);
    assert!(json.get("failure_reason").is_none());
}

#[test]
fn checkpoint_serde_round_trip_preserves_minimal_schema() {
    let expected = CompletionCheckpoint::new("stage-1", "session-1");
    let json = serde_json::to_value(&expected).unwrap();
    let actual: CompletionCheckpoint = serde_json::from_value(json.clone()).unwrap();

    assert_eq!(actual, expected);
    assert_eq!(json.as_object().unwrap().len(), 2);
}

#[test]
fn blocker_fingerprint_ignores_attempt_incidental_fields() {
    let mut baseline = evidence();
    baseline.verification.criteria.push(CriterionResult {
        id: "lint".into(),
        passed: true,
    });
    let expected = CompletionBlocker::from_attempt(&baseline)
        .unwrap()
        .fingerprint;
    let mut changed = baseline;
    changed.phase = CompletionPhase::DaemonRejected;
    changed.evidence_nonce = "different_nonce_1".into();
    changed.observed_at = "2026-09-14T11:00:00Z".into();
    changed.diagnostic_first_line = Some("different diagnostic".into());
    changed.verification.criteria.reverse();

    let actual = CompletionBlocker::from_attempt(&changed)
        .unwrap()
        .fingerprint;
    assert_eq!(actual, expected);
}

#[test]
fn blocker_fingerprint_changes_with_failure_identity() {
    let baseline = evidence();
    let expected = CompletionBlocker::from_attempt(&baseline)
        .unwrap()
        .fingerprint;
    let mut failure_code = baseline.clone();
    failure_code.external_failure_code = Some("daemon_rejected".into());
    let mut commit = baseline.clone();
    commit.commit = "abcdef2".into();
    let mut checks = baseline;
    checks.check_definition_hash = "checks:v2".into();

    let actual = [failure_code, commit, checks].map(|attempt| {
        CompletionBlocker::from_attempt(&attempt)
            .unwrap()
            .fingerprint
    });
    assert!(actual.iter().all(|fingerprint| fingerprint != &expected));
    assert_ne!(actual[0], actual[1]);
    assert_ne!(actual[1], actual[2]);
    assert_ne!(actual[0], actual[2]);
}

#[test]
fn non_verified_attempts_are_not_actionable() {
    let mut tool_failed = evidence();
    tool_failed.phase = CompletionPhase::ToolFailed;
    let mut evidence_missing = evidence();
    evidence_missing.phase = CompletionPhase::EvidenceMissing;
    let mut criterion_failed = evidence();
    criterion_failed.verification.criteria[0].passed = false;
    let mut failure_code_missing = evidence();
    failure_code_missing.external_failure_code = None;

    for attempt in [
        tool_failed,
        evidence_missing,
        criterion_failed,
        failure_code_missing,
    ] {
        assert!(!attempt.is_actionable());
        assert!(CompletionBlocker::from_attempt(&attempt).is_err());
    }
}

#[test]
fn evidence_validation_rejects_invalid_boundaries() {
    let mut empty = evidence();
    empty.stage_id.clear();
    let mut oversized = evidence();
    oversized.exact_command = "x".repeat(MAX_COMMAND_LEN + 1);
    let mut control = evidence();
    control.diagnostic_first_line = Some("line one\nline two".into());
    let mut non_utc = evidence();
    non_utc.observed_at = "2026-09-14T12:00:00+02:00".into();

    let cases = [
        (empty, "stage_id"),
        (oversized, "exact_command"),
        (control, "diagnostic_first_line"),
        (non_utc, "observed_at"),
    ];
    for (attempt, field) in cases {
        let error = attempt.validate().unwrap_err().to_string();
        assert!(error.contains(field));
    }
}

#[test]
fn checkpoint_validation_rejects_more_than_nonce_capacity() {
    let mut checkpoint = CompletionCheckpoint::new("stage-1", "session-1");
    checkpoint.observations = (0..=MAX_EVIDENCE_NONCES)
        .map(|index| NonceObservation {
            evidence_nonce: format!("evidence-{index:08}"),
            identity_digest: "a".repeat(64),
            fingerprint: None,
            phase: CompletionPhase::ToolFailed,
            first_observed_at: "2026-09-14T10:00:00Z".into(),
            last_observed_at: "2026-09-14T10:00:00Z".into(),
            attestation: None,
        })
        .collect();

    assert!(checkpoint.validate().is_err());
}
