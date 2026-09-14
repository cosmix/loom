use std::fs;

use anyhow::Result;
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::models::stage::AcceptanceCriterion;

const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const COMMAND: &str = "/opt/loom stage complete evidence-stage";

fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("handoffs")).unwrap();
    let mut session = Session::new();
    session.id = "evidence-session".to_string();
    let mut stage = Stage::new("Evidence stage".to_string(), None);
    stage.id = "evidence-stage".to_string();
    stage.session = Some(session.id.clone());
    stage.acceptance = vec![AcceptanceCriterion::Simple("true".to_string())];
    (temp, session, stage)
}

fn verified(stage: &Stage, session: &Session) -> CompletionAttemptEvidence {
    verified_evidence(stage, &session.id, COMMIT.to_string(), COMMAND.to_string())
}

fn record_line(evidence: &CompletionAttemptEvidence) -> String {
    format_evidence_record(evidence)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn format_then_parse_round_trips() -> Result<()> {
    let (_, session, stage) = fixture();
    let expected = verified(&stage, &session);

    let actual = parse_evidence_record(&format_evidence_record(&expected)?)?;

    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn forged_earlier_record_is_rejected() {
    let (_, session, stage) = fixture();
    let line = record_line(&verified(&stage, &session));
    let output = format!("{line}\nuntrusted output\n{line}\n{EVIDENCE_EOF_MARKER}\n");

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(
        error.contains("multiple completion evidence records"),
        "{error}"
    );
}

#[test]
fn trailing_text_after_eof_is_rejected() {
    let (_, session, stage) = fixture();
    let output = format!(
        "{} trailing\n",
        format_evidence_record(&verified(&stage, &session))
            .unwrap()
            .trim_end()
    );

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(error.contains("EOF marker"), "{error}");
}

#[test]
fn eof_before_last_line_is_rejected() {
    let (_, session, stage) = fixture();
    let output = format!(
        "{}untrusted trailing line\n",
        format_evidence_record(&verified(&stage, &session)).unwrap()
    );

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(error.contains("not the last line"), "{error}");
}

#[test]
fn missing_record_is_rejected() {
    let output = format!("ordinary output\n{EVIDENCE_EOF_MARKER}\n");

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(error.contains("record is missing"), "{error}");
}

#[test]
fn oversized_json_is_rejected() {
    let json = " ".repeat(MAX_EVIDENCE_BYTES + 1);
    let output = format!("{EVIDENCE_RECORD_PREFIX}{json}\n{EVIDENCE_EOF_MARKER}\n");

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(error.contains("JSON exceeds maximum bytes"), "{error}");
}

#[test]
fn oversized_broker_input_is_rejected() {
    let output = "x".repeat(MAX_BROKER_INPUT_BYTES + 1);

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(
        error.contains("broker input exceeds maximum bytes"),
        "{error}"
    );
}

#[test]
fn unknown_json_key_is_rejected() {
    let (_, session, stage) = fixture();
    let mut value = serde_json::to_value(verified(&stage, &session)).unwrap();
    value["forged_authority"] = json!(true);
    let output = format!("{EVIDENCE_RECORD_PREFIX}{value}\n{EVIDENCE_EOF_MARKER}\n");

    let error = parse_evidence_record(&output).unwrap_err().to_string();

    assert!(
        error.contains("unknown completion evidence field"),
        "{error}"
    );
}

#[test]
fn diagnostic_control_characters_are_stripped_and_text_is_bounded() {
    let (_, session, stage) = fixture();
    let diagnostic = format!("\u{0}bad\u{7}{}\nignored", "x".repeat(MAX_TEXT_LEN + 20));

    let evidence = diagnostic_evidence(
        &stage,
        &session.id,
        COMMIT.to_string(),
        COMMAND.to_string(),
        CompletionPhase::ToolFailed,
        Some(&diagnostic),
    );
    let line = evidence.diagnostic_first_line.unwrap();

    assert_eq!(line.chars().count(), MAX_TEXT_LEN);
    assert!(line.starts_with("bad"));
    assert!(!line.chars().any(char::is_control));
}

#[test]
fn validate_against_rejects_all_identity_and_verification_mismatches() {
    let (_, session, stage) = fixture();
    let evidence = verified(&stage, &session);
    let mut other_stage = stage.clone();
    other_stage.id = "other-stage".to_string();
    assert!(validate_against(&evidence, &other_stage, &session.id, COMMIT, COMMAND).is_err());
    assert!(validate_against(&evidence, &stage, "other-session", COMMIT, COMMAND).is_err());
    assert!(validate_against(&evidence, &stage, &session.id, &"b".repeat(40), COMMAND).is_err());
    assert!(validate_against(&evidence, &stage, &session.id, COMMIT, "/other command").is_err());

    let mut wrong_hash = evidence.clone();
    wrong_hash.check_definition_hash = "b".repeat(64);
    assert!(validate_against(&wrong_hash, &stage, &session.id, COMMIT, COMMAND).is_err());

    let mut wrong_phase = evidence.clone();
    wrong_phase.phase = CompletionPhase::ToolFailed;
    assert!(validate_against(&wrong_phase, &stage, &session.id, COMMIT, COMMAND).is_err());

    let mut failing = evidence;
    failing.verification.criteria[0].passed = false;
    assert!(validate_against(&failing, &stage, &session.id, COMMIT, COMMAND).is_err());
}

#[test]
fn verified_evidence_uses_stable_passed_ids() {
    let (_, session, mut stage) = fixture();
    stage
        .acceptance
        .push(AcceptanceCriterion::Simple("also true".to_string()));
    stage.artifacts.push("loom/src/lib.rs".to_string());

    let evidence = verified(&stage, &session);
    let actual = evidence
        .verification
        .criteria
        .iter()
        .map(|criterion| (criterion.id.as_str(), criterion.passed))
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            ("acceptance-0", true),
            ("acceptance-1", true),
            ("goal-backward", true),
        ]
    );
    assert!(evidence.verification.all_passed());
}

#[test]
fn with_boundary_failure_preserves_attempt_nonce_and_identity() {
    let (_, session, stage) = fixture();
    let evidence = verified(&stage, &session);

    let failed = with_boundary_failure(
        &evidence,
        CompletionPhase::DaemonRejected,
        "ack_lost",
        Some("daemon connection closed"),
    );

    assert_eq!(failed.evidence_nonce, evidence.evidence_nonce);
    assert_eq!(failed.stage_id, evidence.stage_id);
    assert_eq!(failed.session_id, evidence.session_id);
    assert_eq!(failed.commit, evidence.commit);
    assert_eq!(failed.check_definition_hash, evidence.check_definition_hash);
    assert_eq!(failed.exact_command, evidence.exact_command);
    assert_eq!(failed.external_failure_code.as_deref(), Some("ack_lost"));
}

#[test]
fn record_host_fallback_is_idempotent() -> Result<()> {
    let (temp, session, stage) = fixture();
    let evidence = verified(&stage, &session);

    let (_, created) = record_host_fallback(&session, &stage, evidence.clone(), temp.path())?;
    let (_, unchanged) = record_host_fallback(&session, &stage, evidence, temp.path())?;

    assert_eq!(
        (created, unchanged),
        (MergeOutcome::Created, MergeOutcome::Unchanged)
    );
    Ok(())
}

#[test]
fn record_evidence_without_daemon_uses_host_fallback() -> Result<()> {
    let (temp, session, stage) = fixture();
    let evidence = verified(&stage, &session);

    let route = record_evidence(&session, &stage, &evidence, temp.path())?;

    assert_eq!(route, RecordRoute::HostFallback);
    Ok(())
}
