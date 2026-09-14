use std::fs::{self, File};

use serde_json::json;
use tempfile::TempDir;

use super::*;

const JOB: &str = "job-123";

fn write_record(temp: &TempDir, value: serde_json::Value) -> anyhow::Result<std::path::PathBuf> {
    let path = temp.path().join("job.json");
    fs::write(&path, serde_json::to_vec(&value)?)?;
    Ok(path)
}

fn job(status: Option<&str>, phase: Option<&str>) -> CompanionJob {
    CompanionJob {
        id: JOB.to_owned(),
        status: status.map(str::to_owned),
        phase: phase.map(str::to_owned),
        ..CompanionJob::default()
    }
}

fn v1_0_6_job(temp: &TempDir) -> serde_json::Value {
    json!({
        "id": JOB,
        "kind": "task",
        "kindLabel": "rescue",
        "title": "Implement the adapter",
        "workspaceRoot": temp.path(),
        "jobClass": "task",
        "summary": "Implement the adapter",
        "write": true,
        "createdAt": "2026-09-14T10:00:00.000Z",
        "sessionId": "loom.v1:stage-1:session-1:unit-1:inv-0123456789abcdef0123456789abcdef",
        "status": "completed",
        "phase": "done",
        "threadId": "thread-456",
        "turnId": "turn-789",
        "completedAt": "2026-09-14T10:01:00.000Z",
        "errorMessage": null,
        "result": {"text": "done"},
        "request": {
            "cwd": temp.path(),
            "model": "gpt-5.6-sol",
            "effort": "xhigh",
            "prompt": "Implement the adapter",
            "write": true,
            "resumeLast": false,
            "jobId": JOB
        }
    })
}

#[test]
fn reader_accepts_exact_record_and_ignores_extra_fields() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_record(
        &temp,
        json!({
            "id": JOB,
            "status": "running",
            "phase": "starting",
            "threadId": "thread-456",
            "model": "diagnostic-only",
            "rendered": "not evidence"
        }),
    )?;

    let record = read_companion_job(&path, JOB)?;

    assert_eq!(record.id, JOB);
    assert_eq!(record.thread_id.as_deref(), Some("thread-456"));
    assert!(matches!(record.state(), ForwardState::Running));
    Ok(())
}

#[test]
fn reader_rejects_record_id_mismatch() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_record(
        &temp,
        json!({"id": "job-other", "status": "queued", "phase": "queued"}),
    )?;

    assert!(read_companion_job(&path, JOB).is_err());
    Ok(())
}

#[test]
fn reader_rejects_oversized_record() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().join("job.json");
    let file = File::create(&path)?;
    let max_bytes = u64::try_from(MAX_JOB_RECORD_BYTES)?;
    file.set_len(max_bytes + 1)?;

    assert!(read_companion_job(&path, JOB).is_err());
    Ok(())
}

#[test]
fn reader_rejects_unsafe_thread_id() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_record(
        &temp,
        json!({
            "id": JOB,
            "status": "completed",
            "phase": "done",
            "threadId": "../thread"
        }),
    )?;

    assert!(read_companion_job(&path, JOB).is_err());
    Ok(())
}

#[test]
fn reader_pins_companion_v1_0_6_task_schema() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_record(&temp, v1_0_6_job(&temp))?;

    let record = read_companion_job(&path, JOB)?;
    record.validate_v1_0_6()?;

    assert_eq!(
        record
            .request
            .as_ref()
            .map(|request| request.job_id.as_str()),
        Some(JOB)
    );
    assert_eq!(record.turn_id.as_deref(), Some("turn-789"));
    assert!(record.completed_at.is_some());
    Ok(())
}

#[test]
fn v1_0_6_validation_rejects_request_job_mismatch() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let mut value = v1_0_6_job(&temp);
    value["request"]["jobId"] = json!("job-other");
    let path = write_record(&temp, value)?;

    let record = read_companion_job(&path, JOB)?;

    assert!(record.validate_v1_0_6().is_err());
    Ok(())
}

#[test]
fn reader_rejects_noncanonical_completed_at() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let mut value = v1_0_6_job(&temp);
    value["completedAt"] = json!("2026-09-14T10:01:00+00:00");
    let path = write_record(&temp, value)?;

    assert!(read_companion_job(&path, JOB).is_err());
    Ok(())
}

#[test]
fn v1_0_6_validation_rejects_missing_contract_fields() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_record(
        &temp,
        json!({"id": JOB, "status": "running", "phase": "starting"}),
    )?;

    let record = read_companion_job(&path, JOB)?;

    assert!(record.validate_v1_0_6().is_err());
    Ok(())
}

#[test]
fn state_maps_confirmed_companion_vocabulary() {
    assert!(matches!(
        job(Some("completed"), Some("done")).state(),
        ForwardState::Succeeded
    ));
    assert!(matches!(
        job(Some("completed"), Some("starting")).state(),
        ForwardState::Unknown
    ));
    assert!(matches!(
        job(Some("failed"), Some("failed")).state(),
        ForwardState::Failed
    ));
    assert!(matches!(
        job(Some("cancelled"), Some("cancelled")).state(),
        ForwardState::Canceled
    ));
    assert!(matches!(
        job(Some("canceled"), Some("canceled")).state(),
        ForwardState::Canceled
    ));
    assert!(matches!(
        job(Some("queued"), Some("queued")).state(),
        ForwardState::Queued
    ));
    assert!(matches!(
        job(Some("running"), Some("starting")).state(),
        ForwardState::Running
    ));
    assert!(matches!(
        job(Some("starting"), None).state(),
        ForwardState::Running
    ));
    assert!(matches!(
        job(Some("unexpected"), Some("done")).state(),
        ForwardState::Unknown
    ));
    assert!(matches!(
        job(None, Some("starting")).state(),
        ForwardState::Unknown
    ));
}
