use super::*;
use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionPhase, CriterionResult, VerificationCheckpoint,
};

fn write_handoff(path: &Path, handoff: &HandoffV2) {
    fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
}

fn handoff_dir(temp: &tempfile::TempDir) -> PathBuf {
    let dir = temp.path().join("handoffs");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn checkpoint(nonce: &str) -> CompletionCheckpoint {
    let evidence = CompletionAttemptEvidence {
        version: 1,
        stage_id: "stage-1".into(),
        session_id: "session-1".into(),
        commit: "a".repeat(40),
        check_definition_hash: "b".repeat(64),
        exact_command: "loom stage complete s".into(),
        evidence_nonce: nonce.into(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "acceptance".into(),
                passed: true,
            }],
            environment_policy: "trusted-host".into(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some("daemon_offline".into()),
        diagnostic_first_line: None,
        observed_at: "2026-09-14T10:00:00Z".into(),
        attestation: None,
    };
    let mut checkpoint = CompletionCheckpoint::new("stage-1", "session-1");
    checkpoint.record_attempt(&evidence).unwrap();
    checkpoint
}

#[test]
fn finds_an_older_budget_handoff_behind_newer_nonmatches() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    write_handoff(
        &dir.join("stage-1-handoff-001.md"),
        &HandoffV2::new("session-1", "stage-1").with_origin(HandoffOrigin::BudgetExceeded),
    );
    write_handoff(
        &dir.join("stage-1-handoff-002.md"),
        &HandoffV2::new("session-1", "stage-1").with_origin(HandoffOrigin::RedBand),
    );
    fs::write(dir.join("stage-1-handoff-003.md"), "malformed").unwrap();
    write_handoff(
        &dir.join("stage-1-handoff-004.md"),
        &HandoffV2::new("session-2", "stage-1").with_origin(HandoffOrigin::BudgetExceeded),
    );
    write_handoff(
        &dir.join("stage-1-handoff-005.md"),
        &HandoffV2::new("session-1", "stage-1"),
    );

    let found = find_matching_handoff(
        "stage-1",
        "session-1",
        HandoffOrigin::BudgetExceeded,
        temp.path(),
    )
    .unwrap()
    .unwrap();

    assert!(found.ends_with("stage-1-handoff-001.md"));
    assert!(
        find_latest_session_handoff("stage-1", "session-1", temp.path())
            .unwrap()
            .unwrap()
            .ends_with("stage-1-handoff-005.md")
    );
}

#[test]
fn unreadable_numbered_artifact_propagates_uncertainty() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = handoff_dir(&temp).join("stage-1-handoff-001.md");
    fs::create_dir_all(&path).unwrap();

    let error = session_handoffs("stage-1", "session-1", temp.path()).unwrap_err();
    assert!(format!("{error:#}").contains("Failed to read handoff file"));
}

#[test]
fn richer_older_checkpoint_wins_continuation() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    let rich = HandoffV2::new("session-1", "stage-1")
        .with_completion_checkpoint(Some(checkpoint("nonce-000000000000000001")));
    write_handoff(&dir.join("stage-1-handoff-001.md"), &rich);
    write_handoff(
        &dir.join("stage-1-handoff-002.md"),
        &HandoffV2::new("session-1", "stage-1"),
    );

    let found = find_continuation_handoff("stage-1", Some("session-1"), temp.path())
        .unwrap()
        .unwrap();
    assert!(found.ends_with("stage-1-handoff-001.md"));
}

#[test]
fn equal_richness_picks_newest() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    for number in 1..=2 {
        write_handoff(
            &dir.join(format!("stage-1-handoff-{number:03}.md")),
            &HandoffV2::new("session-1", "stage-1"),
        );
    }

    let found = find_continuation_handoff("stage-1", Some("session-1"), temp.path())
        .unwrap()
        .unwrap();
    assert!(found.ends_with("stage-1-handoff-002.md"));
}

#[test]
fn invalid_and_wrong_session_artifacts_are_never_selected() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    write_handoff(
        &dir.join("stage-1-handoff-001.md"),
        &HandoffV2::new("session-1", "stage-1"),
    );
    write_handoff(
        &dir.join("stage-1-handoff-002.md"),
        &HandoffV2::new("session-2", "stage-1"),
    );
    fs::write(dir.join("stage-1-handoff-003.md"), "malformed").unwrap();
    let mismatched = HandoffV2::new("session-1", "stage-1")
        .with_completion_checkpoint(Some(CompletionCheckpoint::new("stage-1", "session-2")));
    write_handoff(&dir.join("stage-1-handoff-004.md"), &mismatched);

    let found = find_continuation_handoff("stage-1", Some("session-1"), temp.path())
        .unwrap()
        .unwrap();

    assert!(found.ends_with("stage-1-handoff-001.md"));
}

#[test]
fn checkpoint_loader_folds_exact_session_artifacts() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    for (number, nonce) in [
        (1, "nonce-000000000000000001"),
        (2, "nonce-000000000000000002"),
    ] {
        let handoff = HandoffV2::new("session-1", "stage-1")
            .with_completion_checkpoint(Some(checkpoint(nonce)));
        write_handoff(
            &dir.join(format!("stage-1-handoff-{number:03}.md")),
            &handoff,
        );
    }

    let loaded = load_session_checkpoint("stage-1", "session-1", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(loaded.repeat_count(), 2);
    assert!(load_session_checkpoint("stage-1", "session-2", temp.path())
        .unwrap()
        .is_none());
}

#[test]
fn no_predecessor_uses_legacy_latest_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    write_handoff(
        &dir.join("stage-1-handoff-001.md"),
        &HandoffV2::new("session-1", "stage-1"),
    );
    fs::write(dir.join("stage-1-handoff-002.md"), "legacy prose").unwrap();

    let found = find_continuation_handoff("stage-1", None, temp.path())
        .unwrap()
        .unwrap();

    assert!(found.ends_with("stage-1-handoff-002.md"));
}
