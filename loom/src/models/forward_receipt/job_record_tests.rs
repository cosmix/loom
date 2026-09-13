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
        thread_id: None,
    }
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
