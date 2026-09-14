#[path = "worker_evidence/setup.rs"]
mod setup;
#[path = "worker_evidence/support.rs"]
mod support;

use serde_json::json;
use std::fs;
use support::{
    assert_exit, assert_watch_rejected, Fixture, AGENT_ID, LOOM_SESSION, PARENT_UUID, STAGE,
    SUCCESSOR_SESSION,
};

#[test]
fn exact_success_records_lifecycle_and_settles() {
    let fixture = Fixture::new("exact-success");
    assert_ne!(PARENT_UUID, LOOM_SESSION);

    let output = fixture.run_stop(&fixture.stop_payload());

    assert_exit(&output, 0);
    let records = fixture.journal_values();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["producer"], "claude_subagent_stop");
    assert_eq!(records[0]["identity"]["parent_session_id"], PARENT_UUID);
    assert_eq!(records[0]["identity"]["loom_session_id"], LOOM_SESSION);
    assert_eq!(
        records[0]["identity"]["transcript_path"],
        fixture.worker.display().to_string()
    );
    let summary = fixture.only_summary();
    assert_eq!(summary["state"], "done");
    assert_eq!(summary["done_evidence"], "lifecycle");
    assert_exit(&fixture.watch(), 0);
}

#[test]
fn loom_session_id_cannot_replace_parent_uuid() {
    let fixture = Fixture::new("parent-is-not-loom-session");
    assert_ne!(PARENT_UUID, LOOM_SESSION);
    let mut payload = fixture.stop_payload();
    payload["session_id"] = json!(LOOM_SESSION);

    let output = fixture.run_stop(&payload);

    assert_exit(&output, 0);
    fixture.assert_no_lifecycle_evidence();
    assert_eq!(fixture.only_summary()["state"], "generating");
    assert_exit(&fixture.watch(), 2);
}

#[test]
fn wrong_active_loom_session_writes_no_evidence_or_heartbeat() {
    let fixture = Fixture::new("wrong-active-session");
    fixture.rebind_stage(SUCCESSOR_SESSION);

    let output = fixture.run_stop(&fixture.stop_payload());

    assert_exit(&output, 0);
    fixture.assert_no_lifecycle_evidence();
    assert!(!fixture.heartbeat_path().exists());
    assert_watch_rejected(&fixture.watch());
}

#[test]
fn transcript_growth_invalidates_stop() {
    let fixture = Fixture::new("stale-stop");
    assert_exit(&fixture.run_stop(&fixture.stop_payload()), 0);
    assert_eq!(fixture.journal_values().len(), 1);

    fixture.append_worker_record("grew after stop");

    let summary = fixture.only_summary();
    assert_eq!(summary["state"], "generating");
    assert!(summary.get("done_evidence").is_none());
    assert_exit(&fixture.watch(), 5);
}

#[test]
fn mismatched_stop_identities_write_nothing() {
    let fixture = Fixture::new("mismatched-identities");

    assert_exit(
        &fixture.run_stop_as(&fixture.stop_payload(), "wrong-stage", LOOM_SESSION),
        0,
    );
    fixture.assert_no_lifecycle_evidence();

    let mut wrong_type = fixture.stop_payload();
    wrong_type["agent_type"] = json!("wrong-agent-type");
    assert_exit(&fixture.run_stop(&wrong_type), 0);
    fixture.assert_no_lifecycle_evidence();

    let mut wrong_worker = fixture.stop_payload();
    wrong_worker["agent_transcript_path"] = json!(fixture.create_worker("other-id"));
    assert_exit(&fixture.run_stop(&wrong_worker), 0);
    fixture.assert_no_lifecycle_evidence();

    let mut wrong_parent = fixture.stop_payload();
    wrong_parent["transcript_path"] = json!(fixture.create_other_parent());
    assert_exit(&fixture.run_stop(&wrong_parent), 0);
    fixture.assert_no_lifecycle_evidence();
    assert_exit(&fixture.watch(), 2);
}

#[test]
fn malformed_duplicate_and_conflicting_deliveries_fail_closed() {
    let fixture = Fixture::new("delivery-semantics");
    assert_exit(&fixture.run_malformed_stop(), 0);
    fixture.assert_no_lifecycle_evidence();

    let payload = fixture.stop_payload();
    assert_exit(&fixture.run_stop(&payload), 0);
    assert_exit(&fixture.run_stop(&payload), 0);
    let records = fixture.journal_values();
    assert!(!records.is_empty());
    assert!(records.iter().all(|record| record == &records[0]));
    assert_eq!(fixture.list_json().as_array().expect("list rows").len(), 1);
    assert_eq!(fixture.only_summary()["state"], "done");
    assert_exit(&fixture.watch(), 0);

    let mut conflict = records[0].clone();
    conflict["observed_at"] = json!("2026-09-13T10:00:02.000Z");
    fixture.append_journal_value(&conflict);

    assert_eq!(fixture.only_summary()["state"], "generating");
    assert_exit(&fixture.watch(), 5);
}

#[test]
fn delayed_predecessor_stop_preserves_successor_heartbeat() {
    let fixture = Fixture::new("delayed-stop");
    fixture.rebind_stage(SUCCESSOR_SESSION);
    let seeded = concat!(
        "{\"stage_id\":\"worker-evidence\",",
        "\"session_id\":\"loom-session-b\",",
        "\"activity\":\"successor\"}\n"
    )
    .as_bytes();
    fixture.seed_heartbeat(seeded);

    let output = fixture.run_stop_as(&fixture.stop_payload(), STAGE, LOOM_SESSION);

    assert_exit(&output, 0);
    fixture.assert_no_lifecycle_evidence();
    assert_eq!(
        fs::read(fixture.heartbeat_path()).expect("read heartbeat"),
        seeded
    );
    assert_eq!(fixture.only_summary()["state"], "generating");
    assert_watch_rejected(&fixture.watch());
}

#[test]
fn teammate_idle_is_recorded_but_never_terminal() {
    let fixture = Fixture::new("teammate-idle");

    let output = fixture.run_idle(&fixture.idle_payload());

    assert_exit(&output, 0);
    let records = fixture.journal_values();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["producer"], "claude_teammate_idle");
    assert_eq!(records[0]["state"], "idle");
    assert_eq!(records[0]["identity"]["teammate_name"], AGENT_ID);
    assert_eq!(records[0]["identity"]["parent_session_id"], PARENT_UUID);
    assert_eq!(fixture.only_summary()["state"], "generating");
    assert_exit(&fixture.watch(), 2);
}
