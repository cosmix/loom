use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use chrono::{TimeZone, Timelike, Utc};
use serde_json::{json, Value};

use super::*;

fn identity() -> ForwardIdentity {
    ForwardIdentity::new("parent-1", "agent_2", "tool.3", "stage-4", "loom5")
        .expect("valid identity")
}

fn observation(state: ForwardState) -> ForwardObservation {
    let identity = identity();
    ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id,
        agent_id: identity.agent_id,
        tool_use_id: identity.tool_use_id,
        stage_id: identity.stage_id,
        loom_session_id: identity.loom_session_id,
        backend: ForwardBackend::Companion,
        backend_id: "job-1".to_string(),
        state,
        observed_at: Utc
            .with_ymd_and_hms(2026, 9, 13, 8, 30, 0)
            .single()
            .expect("valid timestamp"),
        exit_code: None,
        codex_thread_id: None,
        locator: None,
        model: Some("gpt-5".to_string()),
        effort: Some("high".to_string()),
    }
}

fn value() -> Value {
    serde_json::to_value(observation(ForwardState::Running)).expect("serialize fixture")
}

#[test]
fn receipt_id_is_canonical_and_nul_separated() {
    assert_eq!(
        identity().receipt_id(),
        "563e6913529b9445aed0600e4ba42931676df732c4b5588cd9f004f7178c6265"
    );
    let left = ForwardIdentity::new("ab", "c", "tool", "stage", "loom").expect("left");
    let right = ForwardIdentity::new("a", "bc", "tool", "stage", "loom").expect("right");
    assert_ne!(left.receipt_id(), right.receipt_id());
}

#[test]
fn identity_rejects_unsafe_components() {
    for invalid in ["", "-leading", "has/slash", "nonascii-é"] {
        assert!(ForwardIdentity::new(invalid, "agent", "tool", "stage", "loom").is_err());
    }
    let oversized = "a".repeat(129);
    assert!(ForwardIdentity::new(oversized, "agent", "tool", "stage", "loom").is_err());
    assert!(receipts_path(Path::new("/tmp"), "-unsafe").is_err());
}

#[test]
fn decode_line_round_trips_and_rejects_shape_changes() {
    let encoded = observation(ForwardState::Running)
        .encode_line()
        .expect("encode observation");
    assert_eq!(
        ForwardObservation::decode_line(&encoded)
            .expect("decode observation")
            .state,
        ForwardState::Running
    );
    for (field, invalid) in [
        ("schema", json!(2)),
        ("receipt_id", json!("deadbeef")),
        ("backend", json!("other")),
    ] {
        let mut candidate = value();
        candidate[field] = invalid;
        assert!(
            ForwardObservation::decode_line(&candidate.to_string()).is_err(),
            "field: {field}"
        );
    }
    let mut candidate = value();
    candidate["extra"] = json!(true);
    assert!(ForwardObservation::decode_line(&candidate.to_string()).is_err());
    let mut candidate = value();
    candidate
        .as_object_mut()
        .expect("receipt fixture is an object")
        .remove("effort");
    assert!(ForwardObservation::decode_line(&candidate.to_string()).is_err());
}

#[test]
fn decode_line_rejects_unsafe_ids_and_unknown_observations() {
    for (field, invalid) in [
        ("parent_session_id", json!("-bad")),
        ("backend_id", json!("bad/id")),
        ("codex_thread_id", json!("bad id")),
        ("state", json!("unknown")),
    ] {
        let mut candidate = value();
        candidate[field] = invalid;
        assert!(
            ForwardObservation::decode_line(&candidate.to_string()).is_err(),
            "field: {field}"
        );
    }
}

#[test]
fn decode_line_rejects_unbounded_metadata_and_nonterminal_exit() {
    for field in ["model", "effort"] {
        let mut candidate = value();
        candidate[field] = json!("x".repeat(65));
        assert!(
            ForwardObservation::decode_line(&candidate.to_string()).is_err(),
            "field: {field}"
        );
    }
    for locator in [
        "relative/path".to_string(),
        format!("/{}", "x".repeat(4096)),
    ] {
        let mut candidate = value();
        candidate["locator"] = json!(locator);
        assert!(ForwardObservation::decode_line(&candidate.to_string()).is_err());
    }
    let mut candidate = value();
    candidate["exit_code"] = json!(0);
    assert!(ForwardObservation::decode_line(&candidate.to_string()).is_err());
}

#[test]
fn fold_accepts_start_then_terminal() {
    let start = observation(ForwardState::Running);
    let mut terminal = observation(ForwardState::Succeeded);
    terminal.exit_code = Some(0);
    terminal.codex_thread_id = Some("thread-1".to_string());
    terminal.locator = Some("/tmp/job-1.json".to_string());
    terminal.observed_at = Utc
        .with_ymd_and_hms(2026, 9, 13, 8, 31, 0)
        .single()
        .expect("valid timestamp");

    let receipt = fold_observations([start, terminal]).remove(0);
    assert_eq!(receipt.state, ForwardState::Succeeded);
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.codex_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(receipt.locator.as_deref(), Some("/tmp/job-1.json"));
    assert_eq!(receipt.observed_at.minute(), 31);
}

#[test]
fn fold_rejects_terminal_first_and_backend_identity_changes() {
    let mut terminal = observation(ForwardState::Failed);
    terminal.exit_code = Some(1);
    assert_eq!(
        fold_observations([terminal]).remove(0).state,
        ForwardState::Unknown
    );

    let start = observation(ForwardState::Running);
    let mut changed = observation(ForwardState::Running);
    changed.backend_id = "job-2".to_string();
    assert_eq!(
        fold_observations([start, changed]).remove(0).state,
        ForwardState::Unknown
    );
    let mut changed = observation(ForwardState::Running);
    changed.backend = ForwardBackend::Direct;
    assert_eq!(
        fold_observations([observation(ForwardState::Running), changed])
            .remove(0)
            .state,
        ForwardState::Unknown
    );
}

#[test]
fn fold_handles_terminal_conflicts_and_idempotent_repeats() {
    let start = observation(ForwardState::Running);
    let mut succeeded = observation(ForwardState::Succeeded);
    succeeded.exit_code = Some(0);
    let repeated_start = start.clone();
    let repeated_terminal = succeeded.clone();
    let receipt =
        fold_observations([start, succeeded, repeated_start, repeated_terminal]).remove(0);
    assert_eq!(receipt.state, ForwardState::Succeeded);

    let mut failed = observation(ForwardState::Failed);
    failed.exit_code = Some(1);
    let receipt = fold_observations([
        observation(ForwardState::Running),
        observation_with_exit(ForwardState::Succeeded, 0),
        failed,
    ])
    .remove(0);
    assert_eq!(receipt.state, ForwardState::Unknown);
}

#[test]
fn fold_rejects_thread_and_locator_conflicts() {
    let mut first = observation(ForwardState::Running);
    first.codex_thread_id = Some("thread-1".to_string());
    first.locator = Some("/tmp/job-1.json".to_string());
    let mut thread_changed = first.clone();
    thread_changed.codex_thread_id = Some("thread-2".to_string());
    assert_eq!(
        fold_observations([first.clone(), thread_changed])
            .remove(0)
            .state,
        ForwardState::Unknown
    );

    let mut locator_changed = first.clone();
    locator_changed.locator = Some("/tmp/job-2.json".to_string());
    assert_eq!(
        fold_observations([first, locator_changed]).remove(0).state,
        ForwardState::Unknown
    );
}

#[test]
fn fold_enforces_direct_backend_rules() {
    let mut valid = observation(ForwardState::Running);
    valid.backend = ForwardBackend::Direct;
    valid.backend_id = "thread-1".to_string();
    valid.codex_thread_id = Some("thread-1".to_string());
    assert_eq!(
        fold_observations([valid.clone()]).remove(0).state,
        ForwardState::Running
    );

    let mut wrong_thread = valid.clone();
    wrong_thread.codex_thread_id = Some("thread-2".to_string());
    assert_eq!(
        fold_observations([wrong_thread]).remove(0).state,
        ForwardState::Unknown
    );
    let mut located = valid;
    located.locator = Some("/tmp/job.json".to_string());
    assert_eq!(
        fold_observations([located]).remove(0).state,
        ForwardState::Unknown
    );
}

#[test]
fn locator_never_serializes() {
    let mut start = observation(ForwardState::Running);
    start.locator = Some("/tmp/private-job.json".to_string());
    let receipt = fold_observations([start]).remove(0);
    let encoded = serde_json::to_value(receipt).expect("serialize receipt");
    assert!(encoded.get("locator").is_none());
}

#[test]
fn loader_handles_missing_symlink_and_malformed_lines() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let missing = load_receipts(&temp.path().join("missing.jsonl")).expect("missing is empty");
    assert!(missing.receipts.is_empty());

    let target = temp.path().join("target.jsonl");
    fs::write(&target, "").expect("write target");
    let link = temp.path().join("link.jsonl");
    symlink(&target, &link).expect("create symlink");
    assert!(load_receipts(&link).is_err());

    let data = temp.path().join("receipts.jsonl");
    let line = observation(ForwardState::Running)
        .encode_line()
        .expect("encode");
    fs::write(&data, format!("{line}\nnot-json\n")).expect("write receipts");
    let loaded = load_receipts(&data).expect("load receipts");
    assert_eq!((loaded.receipts.len(), loaded.malformed), (1, 1));
}

#[test]
fn loader_marks_byte_and_line_limits_truncated() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let bytes_path = temp.path().join("bytes.jsonl");
    fs::write(&bytes_path, vec![b'x'; 8 * 1024 * 1024 + 1]).expect("write oversized bytes");
    assert!(load_receipts(&bytes_path).expect("load bytes").truncated);

    let lines_path = temp.path().join("lines.jsonl");
    fs::write(&lines_path, "\n".repeat(20_001)).expect("write oversized lines");
    let loaded = load_receipts(&lines_path).expect("load lines");
    assert!(loaded.truncated);
    assert_eq!(loaded.malformed, 20_000);
}

fn observation_with_exit(state: ForwardState, exit_code: i32) -> ForwardObservation {
    let mut observation = observation(state);
    observation.exit_code = Some(exit_code);
    observation
}
