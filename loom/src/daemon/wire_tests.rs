use super::*;
use crate::daemon::protocol::{CompletionSummary, DaemonConfig, StageCompletionInfo, StageInfo};
use crate::models::stage::StageStatus;
use crate::models::worktree::WorktreeStatus;
use chrono::Utc;
use std::io::{Cursor, Read};

struct CountingReader<R> {
    inner: R,
    bytes_read: usize,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.bytes_read += count;
        Ok(count)
    }
}

#[test]
fn request_round_trip_includes_preface() {
    let mut buffer = Vec::new();
    let request = Request::Ping {
        auth_token: "test-token".to_string(),
    };
    write_message(&mut buffer, &request).unwrap();

    let decoded: Request = read_message(&mut Cursor::new(buffer)).unwrap();

    assert!(matches!(decoded, Request::Ping { auth_token } if auth_token == "test-token"));
}

#[test]
fn response_round_trip_uses_bounded_frame() {
    let mut buffer = Vec::new();
    write_message(&mut buffer, &Response::Pong).unwrap();

    let decoded: Response = read_message(&mut Cursor::new(buffer)).unwrap();

    assert!(matches!(decoded, Response::Pong));
}

#[test]
fn status_update_round_trip() {
    let now = Utc::now();
    let response = Response::StatusUpdate {
        stages_executing: vec![stage_info("stage-1", StageStatus::Executing, now)],
        stages_pending: vec![stage_info("stage-2", StageStatus::WaitingForDeps, now)],
        stages_completed: vec![stage_info("stage-0", StageStatus::Completed, now)],
        stages_blocked: vec![],
    };
    let mut buffer = Vec::new();
    write_message(&mut buffer, &response).unwrap();

    let decoded: Response = read_message(&mut Cursor::new(buffer)).unwrap();

    match decoded {
        Response::StatusUpdate {
            stages_executing, ..
        } => assert_eq!(stages_executing[0].id, "stage-1"),
        _ => panic!("expected status update"),
    }
}

fn stage_info(id: &str, status: StageStatus, now: chrono::DateTime<Utc>) -> StageInfo {
    StageInfo {
        id: id.to_string(),
        name: id.to_string(),
        session_pid: None,
        started_at: now,
        completed_at: None,
        worktree_status: Some(WorktreeStatus::Active),
        status,
        merged: false,
        dependencies: vec![],
        model: "test".to_string(),
        cleanup_warning: None,
    }
}

#[test]
fn invalid_preface_is_rejected_before_huge_frame_length_is_read() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"NOPE");
    bytes.extend_from_slice(&[REQUEST_PREFACE_VERSION, 1, 0, 1]);
    bytes.push(b'x');
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    let mut reader = CountingReader {
        inner: Cursor::new(bytes),
        bytes_read: 0,
    };

    let error = read_request_preface(&mut reader).unwrap_err();

    assert!(error.to_string().contains("Invalid request"));
    assert_eq!(reader.bytes_read, REQUEST_PREFACE_HEADER_BYTES);
}

#[test]
fn oversized_preface_credential_does_not_read_credential_or_frame() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&REQUEST_PREFACE_MAGIC);
    bytes.extend_from_slice(&[
        REQUEST_PREFACE_VERSION,
        1,
        ((MAX_CREDENTIAL_BYTES + 1) >> 8) as u8,
        (MAX_CREDENTIAL_BYTES + 1) as u8,
    ]);
    bytes.resize(
        REQUEST_PREFACE_HEADER_BYTES + MAX_CREDENTIAL_BYTES + 1,
        b'x',
    );
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    let mut reader = CountingReader {
        inner: Cursor::new(bytes),
        bytes_read: 0,
    };

    let error = read_request_preface(&mut reader).unwrap_err();

    assert!(error.to_string().contains("credential length"));
    assert_eq!(reader.bytes_read, REQUEST_PREFACE_HEADER_BYTES);
}

#[test]
fn oversized_request_body_length_is_rejected_before_allocation() {
    let mut buffer = Vec::new();
    write_request_preface(&mut buffer, Capability::User, "token").unwrap();
    buffer.extend_from_slice(&((MAX_REQUEST_BYTES + 1) as u32).to_be_bytes());
    let mut cursor = Cursor::new(buffer);

    let _preface = read_request_preface(&mut cursor).unwrap();
    let error = read_request_length(&mut cursor).unwrap_err();

    assert!(error.to_string().contains("configured limit"));
}

#[test]
fn oversized_credential_and_frames_are_rejected_on_write() {
    let oversized_credential = Request::Ping {
        auth_token: "x".repeat(MAX_CREDENTIAL_BYTES + 1),
    };
    assert!(write_message(&mut Vec::new(), &oversized_credential).is_err());

    let oversized_request = Request::DisputeCriteria {
        auth_token: "token".to_string(),
        stage_id: "stage".to_string(),
        criterion_index: 0,
        reason: "x".repeat(MAX_REQUEST_BYTES),
        evidence_commit: None,
        failure_output: None,
    };
    assert!(write_message(&mut Vec::new(), &oversized_request).is_err());

    let oversized_response = Response::LogLine {
        line: "x".repeat(MAX_RESPONSE_BYTES),
    };
    assert!(write_message(&mut Vec::new(), &oversized_response).is_err());
}

#[test]
fn request_debug_output_redacts_credentials_and_evidence() {
    let request = Request::DisputeCriteria {
        auth_token: "proof-secret".to_string(),
        stage_id: "stage".to_string(),
        criterion_index: 0,
        reason: "private reason".to_string(),
        evidence_commit: None,
        failure_output: Some("private output".to_string()),
    };

    let output = format!("{request:?}");
    assert!(!output.contains("proof-secret"));
    assert!(!output.contains("private reason"));
    assert!(!output.contains("private output"));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn daemon_config_default_is_safe() {
    let config = DaemonConfig::default();

    assert!(!config.manual_mode);
    assert!(config.max_parallel.is_none());
    assert!(config.watch_mode);
    assert!(config.auto_merge);
}

#[test]
fn orchestration_complete_round_trip() {
    let response = Response::OrchestrationComplete {
        summary: CompletionSummary {
            total_duration_secs: 120,
            stages: vec![StageCompletionInfo {
                id: "stage-1".to_string(),
                name: "First Stage".to_string(),
                status: StageStatus::Completed,
                duration_secs: Some(60),
                execution_secs: None,
                retry_count: 0,
                merged: true,
                dependencies: vec![],
            }],
            success_count: 1,
            failure_count: 0,
            plan_path: "doc/plans/PLAN-test.md".to_string(),
        },
    };
    let mut buffer = Vec::new();
    write_message(&mut buffer, &response).unwrap();

    let decoded: Response = read_message(&mut Cursor::new(buffer)).unwrap();

    match decoded {
        Response::OrchestrationComplete { summary } => {
            assert_eq!(summary.total_duration_secs, 120);
            assert_eq!(summary.stages[0].id, "stage-1");
            assert_eq!(summary.plan_path, "doc/plans/PLAN-test.md");
        }
        _ => panic!("expected orchestration complete"),
    }
}
