use super::*;
use anyhow::Result;
use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct ReceiptEnvironment(Vec<(&'static str, Option<OsString>)>);

impl Drop for ReceiptEnvironment {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            if let Some(value) = value {
                std::env::set_var(*name, value);
            } else {
                std::env::remove_var(*name);
            }
        }
    }
}

fn without_receipt_environment() -> ReceiptEnvironment {
    let names = ["LOOM_WORK_DIR", "LOOM_SESSION_ID", "LOOM_STAGE_ID"];
    let saved = names
        .into_iter()
        .map(|name| (name, std::env::var_os(name)))
        .collect();
    for name in names {
        std::env::remove_var(name);
    }
    ReceiptEnvironment(saved)
}

struct Fixture {
    temp: TempDir,
    source: PathBuf,
    transcript: PathBuf,
    store: ReceiptStore,
}

impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::Builder::new()
            .prefix("loom-read-receipts-")
            .tempdir_in(std::env::temp_dir())?;
        let source = temp.path().join("source.rs");
        let transcript = temp.path().join("transcript.jsonl");
        fs::write(&source, "first\nsecond\nthird\n")?;
        Ok(Self {
            store: ReceiptStore::at(temp.path().join("receipts")),
            temp,
            source,
            transcript,
        })
    }

    fn payload(&self, session: &str, agent: &str, offset: Option<u64>) -> Payload {
        payload(
            &self.source,
            &self.transcript,
            session,
            agent,
            offset,
            "read-one",
            "Read",
        )
    }

    fn write_result(
        &self,
        payload: &Payload,
        content: serde_json::Value,
        is_error: bool,
    ) -> Result<()> {
        fs::write(
            &self.transcript,
            transcript_pair(payload, content, is_error),
        )?;
        Ok(())
    }

    fn receipt_files(&self) -> Result<Vec<PathBuf>> {
        let files = fs::read_dir(&self.store.root)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("receipt-"))
            })
            .collect();
        Ok(files)
    }
}

fn payload(
    source: &Path,
    transcript: &Path,
    session: &str,
    agent: &str,
    offset: Option<u64>,
    tool_id: &str,
    tool_name: &str,
) -> Payload {
    let mut input = json!({ "file_path": source });
    if let Some(offset) = offset {
        input["offset"] = json!(offset);
        input["limit"] = json!(1);
    }
    let raw = json!({
        "session_id": session,
        "agent_id": agent,
        "cwd": source.parent().unwrap(),
        "tool_name": tool_name,
        "tool_input": input,
        "transcript_path": transcript,
        "tool_use_id": tool_id,
    });
    Payload::parse(&raw.to_string()).unwrap()
}

fn transcript_pair(payload: &Payload, content: serde_json::Value, is_error: bool) -> String {
    let tool = json!({
        "type": "tool_use",
        "id": payload.tool_use_id,
        "name": "Read",
        "input": payload.input,
    });
    let result = json!({
        "type": "tool_result",
        "tool_use_id": payload.tool_use_id,
        "content": content,
        "is_error": is_error,
    });
    let assistant = json!({ "type": "assistant", "message": { "content": [tool] } });
    let user = json!({ "type": "user", "message": { "content": [result] } });
    format!("{assistant}\n{user}\n")
}

fn prove(fixture: &Fixture, payload: &Payload, content: serde_json::Value) -> Result<()> {
    fixture.store.prepare(payload)?;
    fixture.write_result(payload, content, false)?;
    fixture.store.complete(payload)
}

#[test]
fn receipt_is_atomic_deduplicated_and_hides_result_text() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);
    let hostile = "line one\nline\ttwo";

    prove(&fixture, &payload, json!(hostile))?;
    prove(&fixture, &payload, json!(hostile))?;

    let files = fixture.receipt_files()?;
    assert_eq!(files.len(), 1);
    assert_eq!(fixture.store.check(&payload)?, Some(1));
    let stored = fs::read_to_string(&files[0])?;
    assert!(!stored.contains(hostile));
    assert!(!stored.contains("line one"));
    Ok(())
}

#[test]
fn receipt_requires_matching_session_agent_path_range_and_epoch() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", Some(0));

    prove(&fixture, &payload, json!("first"))?;

    assert_eq!(fixture.store.check(&payload)?, Some(1));
    assert!(fixture
        .store
        .check(&fixture.payload("session-b", "agent-a", Some(0)))?
        .is_none());
    assert!(fixture
        .store
        .check(&fixture.payload("session-a", "agent-b", Some(0)))?
        .is_none());
    assert!(fixture
        .store
        .check(&fixture.payload("session-a", "agent-a", Some(1)))?
        .is_none());
    fixture.store.rotate_epoch(&payload.session_id)?;
    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn source_edit_between_prepare_and_complete_discards_intent() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    fixture.store.prepare(&payload)?;
    fs::write(&fixture.source, "changed\n")?;
    fixture.write_result(&payload, json!("first"), false)?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    assert!(fixture.receipt_files()?.is_empty());
    Ok(())
}

#[test]
fn changed_range_or_epoch_during_complete_never_creates_receipt() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = fixture.payload("session-a", "agent-a", Some(0));
    let changed_range = fixture.payload("session-a", "agent-a", Some(1));

    fixture.store.prepare(&initial)?;
    fixture.write_result(&changed_range, json!("second"), false)?;
    fixture.store.complete(&changed_range)?;
    fixture.store.rotate_epoch(&initial.session_id)?;
    fixture.write_result(&initial, json!("first"), false)?;
    fixture.store.complete(&initial)?;

    assert!(fixture.store.check(&initial)?.is_none());
    assert!(fixture.receipt_files()?.is_empty());
    Ok(())
}

#[test]
fn source_hash_change_invalidates_an_existing_receipt() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    prove(&fixture, &payload, json!("first"))?;
    fs::write(&fixture.source, "replaced\n")?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn duplicate_matching_reads_are_ambiguous() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);
    let pair = transcript_pair(&payload, json!("first"), false);

    fixture.store.prepare(&payload)?;
    fs::write(&fixture.transcript, format!("{pair}{pair}"))?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn duplicate_or_changed_results_are_ambiguous() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);
    let pair = transcript_pair(&payload, json!("first"), false);
    let changed = json!({ "type": "user", "message": { "content": [{
        "type": "tool_result", "tool_use_id": "read-one", "content": "changed", "is_error": false
    }] } });

    fixture.store.prepare(&payload)?;
    fs::write(&fixture.transcript, format!("{pair}{changed}\n"))?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn failures_and_malformed_transcripts_never_prove() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    fixture.store.prepare(&payload)?;
    fixture.write_result(&payload, json!("failure"), true)?;
    fixture.store.complete(&payload)?;
    fs::write(&fixture.transcript, "{ malformed\n")?;
    fixture.store.complete(&payload)?;

    fs::remove_file(&fixture.transcript)?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn unsafe_or_truncated_transcript_and_non_read_prepare_are_noops() -> Result<()> {
    let fixture = Fixture::new()?;
    let receipt_payload = fixture.payload("session-a", "agent-a", None);
    let link = fixture.temp.path().join("transcript-link.jsonl");

    fs::write(
        &fixture.transcript,
        transcript_pair(&receipt_payload, json!("text"), false),
    )?;
    std::os::unix::fs::symlink(&fixture.transcript, &link)?;
    let unsafe_payload = payload(
        &fixture.source,
        &link,
        "session-a",
        "agent-a",
        None,
        "read-one",
        "Read",
    );
    fixture.store.prepare(&unsafe_payload)?;
    fixture.store.complete(&unsafe_payload)?;
    fs::write(&fixture.transcript, "x".repeat(128 * 1024 + 1))?;
    fixture.store.complete(&receipt_payload)?;

    let non_read = payload(
        &fixture.source,
        &fixture.transcript,
        "session-a",
        "agent-a",
        None,
        "read-one",
        "Bash",
    );
    fixture.store.prepare(&non_read)?;
    assert!(fixture.store.check(&receipt_payload)?.is_none());
    Ok(())
}

#[test]
fn prepare_without_a_readable_source_creates_no_state() -> Result<()> {
    let fixture = Fixture::new()?;
    let missing = fixture.temp.path().join("missing.rs");
    let payload = payload(
        &missing,
        &fixture.transcript,
        "session-a",
        "agent-a",
        None,
        "read-one",
        "Read",
    );

    fixture.store.prepare(&payload)?;

    assert!(!fixture.store.root.exists());
    Ok(())
}

#[test]
fn media_receipt_is_recorded_but_never_proven_for_text_repeat() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    prove(
        &fixture,
        &payload,
        json!([{ "type": "image", "source": "not persisted" }]),
    )?;

    let files = fixture.receipt_files()?;
    assert_eq!(files.len(), 1);
    assert!(fs::read_to_string(&files[0])?.contains("\"content_class\":\"media\""));
    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[path = "tests_extra.rs"]
mod extra;
