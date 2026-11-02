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
fn red_band_context_uses_newest_matching_handoff() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    for (number, tokens) in [(1, 100), (2, 200)] {
        let handoff = HandoffV2::new("session-1", "stage-1")
            .with_origin(HandoffOrigin::RedBand)
            .with_context_tokens(tokens);
        write_handoff(
            &dir.join(format!("stage-1-handoff-{number:03}.md")),
            &handoff,
        );
    }
    let lookup = |tokens| {
        find_matching_handoff_with_context(
            "stage-1",
            "session-1",
            HandoffOrigin::RedBand,
            tokens,
            temp.path(),
        )
        .unwrap()
    };
    assert!(!lookup(100));
    assert!(lookup(200));
}

#[test]
fn bounded_handoff_reads_reject_special_files_and_preserve_uncertainty() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    let fifo = dir.join("stage-1-handoff-001.md");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: name is a valid, NUL-terminated path for the duration of the call.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (tx, rx) = mpsc::channel();
    let root = temp.path().to_owned();
    std::thread::spawn(move || {
        tx.send(session_handoffs("stage-1", "session-1", &root).is_err())
            .unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap());
    fs::remove_file(&fifo).unwrap();

    let outside = tempfile::NamedTempFile::new().unwrap();
    symlink(outside.path(), &fifo).unwrap();
    assert!(session_handoffs("stage-1", "session-1", temp.path()).is_err());
    fs::remove_file(&fifo).unwrap();
    fs::create_dir(&fifo).unwrap();
    assert!(session_handoffs("stage-1", "session-1", temp.path()).is_err());
    fs::remove_dir(&fifo).unwrap();
    assert!(read_handoff_artifact(temp.path(), &fifo).is_err());

    fs::write(&fifo, vec![b'x'; MAX_HANDOFF_BYTES + 1]).unwrap();
    assert!(session_handoffs("stage-1", "session-1", temp.path())
        .unwrap()
        .is_empty());
    write_handoff(&fifo, &HandoffV2::new("session-1", "stage-1"));
    assert_eq!(
        session_handoffs("stage-1", "session-1", temp.path())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn handoff_reads_allow_work_root_symlink_but_reject_symlinked_handoffs() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    let alias = temp.path().join("work");
    symlink(&root, &alias).unwrap();
    let dir = root.join("handoffs");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("stage-1-handoff-001.md");
    write_handoff(&path, &HandoffV2::new("session-1", "stage-1"));
    assert_eq!(
        session_handoffs("stage-1", "session-1", &alias)
            .unwrap()
            .len(),
        1
    );
    fs::rename(&dir, outside.join("handoffs")).unwrap();
    symlink(outside.join("handoffs"), &dir).unwrap();
    assert!(session_handoffs("stage-1", "session-1", &alias).is_err());
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
fn trusted_loader_keeps_signed_evidence_and_receipt_in_either_filename_order() {
    use crate::handoff::completion::attestation_key;
    for signed_first in [true, false] {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = handoff_dir(&temp);
        let key = attestation_key(temp.path()).unwrap();
        let mut authentic = checkpoint("nonce-000000000000000001");
        let receipt = signed_receipt(&key);
        authentic.record_accepted(receipt.clone()).unwrap();
        for observation in &mut authentic.observations {
            observation.attestation =
                Some(key.sign_observation("stage-1", "session-1", observation));
        }
        let latest = authentic.latest.as_mut().unwrap();
        latest.attestation = Some(key.sign_evidence(latest));

        let mut forged = checkpoint("nonce-000000000000000002");
        forged.latest.as_mut().unwrap().observed_at = "2026-09-15T10:00:00Z".into();
        forged.conflict = true;
        forged.capacity_exhausted = true;
        for number in 3..=33 {
            let filler = checkpoint(&format!("nonce-{number:020}"));
            forged.observations.extend(filler.observations);
        }
        let pair = if signed_first {
            [authentic.clone(), forged]
        } else {
            [forged, authentic.clone()]
        };
        for (number, checkpoint) in pair.into_iter().enumerate() {
            write_handoff(
                &dir.join(format!("stage-1-handoff-{:03}.md", number + 1)),
                &HandoffV2::new("session-1", "stage-1")
                    .with_completion_checkpoint(Some(checkpoint)),
            );
        }
        let trusted = load_trusted_session_checkpoint("stage-1", "session-1", temp.path())
            .unwrap()
            .unwrap();
        assert_eq!(trusted.latest, authentic.latest);
        assert_eq!(trusted.accepted, Some(receipt));
        assert!(!trusted.conflict);
        assert!(!trusted.capacity_exhausted);
        assert_eq!(trusted.observations.len(), 1);
    }
}

fn signed_receipt(
    key: &crate::handoff::completion::AttestationKey,
) -> crate::handoff::schema::AcceptedReceipt {
    let mut receipt = crate::handoff::schema::AcceptedReceipt {
        evidence_nonce: "nonce-000000000000000001".into(),
        completion_nonce: "completion-00000000000000001".into(),
        commit: "a".repeat(40),
        attestation: None,
    };
    receipt.attestation = Some(key.sign_receipt("stage-1", "session-1", &receipt));
    receipt
}

#[test]
fn trusted_loader_fails_closed_on_signed_identity_conflict() {
    use crate::handoff::completion::attestation_key;
    let temp = tempfile::TempDir::new().unwrap();
    let dir = handoff_dir(&temp);
    let key = attestation_key(temp.path()).unwrap();
    for (number, commit) in [(1, 'a'), (2, 'c')] {
        let mut checkpoint = checkpoint("nonce-000000000000000001");
        checkpoint.latest.as_mut().unwrap().commit = commit.to_string().repeat(40);
        checkpoint.blocker = Some(
            crate::handoff::schema::CompletionBlocker::from_attempt(
                checkpoint.latest.as_ref().unwrap(),
            )
            .unwrap(),
        );
        checkpoint.observations[0].identity_digest =
            checkpoint.latest.as_ref().unwrap().identity_digest();
        checkpoint.observations[0].fingerprint = checkpoint
            .blocker
            .as_ref()
            .map(|blocker| blocker.fingerprint.clone());
        for observation in &mut checkpoint.observations {
            observation.attestation =
                Some(key.sign_observation("stage-1", "session-1", observation));
        }
        let latest = checkpoint.latest.as_mut().unwrap();
        latest.attestation = Some(key.sign_evidence(latest));
        write_handoff(
            &dir.join(format!("stage-1-handoff-{number:03}.md")),
            &HandoffV2::new("session-1", "stage-1").with_completion_checkpoint(Some(checkpoint)),
        );
    }
    let trusted = load_trusted_session_checkpoint("stage-1", "session-1", temp.path())
        .unwrap()
        .unwrap();
    assert!(trusted.conflict);
    assert!(!trusted.is_actionable());
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
