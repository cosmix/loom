use super::super::{invoke, rotate_epoch_for_session, Mode, Payload, ReceiptStore};
use super::*;
use anyhow::Result;
use serde_json::json;
use serial_test::serial;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn receipt_counts_distinct_correlated_tool_uses() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = fixture.payload("session-a", "agent-a", None);
    let second = payload(
        &fixture.source,
        &fixture.transcript,
        "session-a",
        "agent-a",
        None,
        "read-two",
        "Read",
    );

    prove(&fixture, &first, json!("first"))?;
    prove(&fixture, &second, json!("first"))?;
    prove(&fixture, &second, json!("first"))?;

    assert_eq!(fixture.store.check(&first)?, Some(2));
    assert_eq!(fixture.receipt_files()?.len(), 1);
    Ok(())
}

#[test]
fn duplicate_prepares_discard_ambiguous_pending_intents() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    fixture.store.prepare(&payload)?;
    fixture.store.prepare(&payload)?;
    fixture.write_result(&payload, json!("first"), false)?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    assert!(fixture.receipt_files()?.is_empty());
    Ok(())
}

#[test]
fn compaction_between_prepare_and_complete_discards_the_intent() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    fixture.store.prepare(&payload)?;
    fixture.store.rotate_epoch(&payload.session_id)?;
    fixture.write_result(&payload, json!("first"), false)?;
    fixture.store.complete(&payload)?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn compaction_after_a_receipt_removes_its_proven_status() -> Result<()> {
    let fixture = Fixture::new()?;
    let payload = fixture.payload("session-a", "agent-a", None);

    prove(&fixture, &payload, json!("first"))?;
    fixture.store.rotate_epoch(&payload.session_id)?;

    assert!(fixture.store.check(&payload)?.is_none());
    Ok(())
}

#[test]
fn pages_only_reads_have_a_distinct_normalized_range() {
    let pages = super::super::source::range(&json!({ "pages": "1-2" }));

    assert_eq!(pages, Some("pages:1-2".to_owned()));
    assert_ne!(pages, Some("full".to_owned()));
}

#[test]
#[serial]
fn fallback_store_is_private_and_scoped_to_the_payload_session() -> Result<()> {
    let _environment = without_receipt_environment();
    let fixture = Fixture::new()?;
    let first = fixture.payload("fallback-a", "agent-a", None);
    let second = fixture.payload("fallback-b", "agent-a", None);
    let first_store = ReceiptStore::for_payload(&first).unwrap();
    let second_store = ReceiptStore::for_payload(&second).unwrap();

    assert_ne!(first_store.root, second_store.root);
    assert_eq!(
        fs::metadata(&first_store.root)?.permissions().mode() & 0o077,
        0
    );
    fs::set_permissions(&first_store.root, fs::Permissions::from_mode(0o755))?;

    assert!(ReceiptStore::for_payload(&first).is_none());

    fs::set_permissions(&first_store.root, fs::Permissions::from_mode(0o700))?;
    fs::remove_dir_all(&first_store.root)?;
    fs::remove_dir_all(&second_store.root)?;
    Ok(())
}

#[test]
#[serial]
fn invoke_prepares_and_checks_in_a_bare_temp_cwd() -> Result<()> {
    let _environment = without_receipt_environment();
    let fixture = Fixture::new()?;
    let raw = json!({
        "session_id": "bare-cwd-session",
        "agent_id": "agent-a",
        "cwd": fixture.temp.path(),
        "tool_name": "Read",
        "tool_input": { "file_path": fixture.source },
        "transcript_path": fixture.transcript,
        "tool_use_id": "read-one",
    })
    .to_string();
    let payload = Payload::parse(&raw).unwrap();
    let store = ReceiptStore::for_payload(&payload).unwrap();

    assert!(invoke(Mode::Prepare, &raw).is_none());
    fixture.write_result(&payload, json!("first"), false)?;
    assert!(invoke(Mode::Complete, &raw).is_none());

    assert_eq!(invoke(Mode::Check, &raw), Some(1));
    rotate_epoch_for_session("bare-cwd-session");
    assert!(invoke(Mode::Check, &raw).is_none());
    fs::remove_dir_all(store.root)?;
    Ok(())
}
