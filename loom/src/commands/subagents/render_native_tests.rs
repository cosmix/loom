use super::*;
use crate::native_worker::model::{NativeEvent, NativeManifest, NATIVE_DEADLINE_SECS};
use crate::native_worker::store::{self, EventLog};
use chrono::{Duration, TimeZone};
use std::path::PathBuf;
use tempfile::TempDir;

fn seed(work: &Path, unit: &str, serial: u8) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: "stage-a".into(),
        unit_id: unit.into(),
        invocation_id: format!("inv-{serial:032x}"),
        owner_session_id: "session".into(),
        workspace_root: PathBuf::from("/workspace"),
        brief_path: PathBuf::from("/workspace/brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc.timestamp_opt(i64::from(serial), 0).unwrap(),
    };
    store::create_invocation(work, &manifest, b"brief").unwrap()
}

fn append(dir: &Path, events: Vec<NativeEvent>) {
    let mut log = EventLog::open(dir).unwrap();
    for event in events {
        log.append(event).unwrap();
    }
}

fn terminal(outcome: Outcome, reason: &str) -> NativeEvent {
    NativeEvent::Terminal {
        outcome,
        reason: reason.into(),
    }
}

fn released() -> NativeEvent {
    NativeEvent::OwnershipReleased {
        by: "supervisor".into(),
    }
}

fn with_report(dir: &Path, text: &str, complete: bool) {
    let info = store::write_report(dir, text).unwrap();
    append(
        dir,
        vec![NativeEvent::ReportStored {
            bytes: info.bytes,
            sha256: info.sha256,
            complete,
        }],
    );
}

fn rows(work: &Path) -> Vec<SubagentSummary> {
    summaries_for_stage(work, "stage-a", Utc::now() + Duration::seconds(1))
}

#[test]
fn running_worker_is_one_forward_wait_row() {
    let work = TempDir::new().unwrap();
    seed(work.path(), "unit-a", 1);
    let rows = rows(work.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].agent_id, "codex:unit-a");
    assert_eq!(rows[0].state, SubagentState::ForwardWait);
    assert_eq!(rows[0].model.as_deref(), Some("gpt-6.1-terra (requested)"));
    assert_eq!(rows[0].final_report, None);
    assert!(rows[0].idle_secs >= 0);
}

#[test]
fn finished_worker_row_carries_its_report() {
    let work = TempDir::new().unwrap();
    let dir = seed(work.path(), "unit-a", 1);
    with_report(&dir, "all done", true);
    append(&dir, vec![terminal(Outcome::Succeeded, "done"), released()]);
    let row = rows(work.path()).remove(0);
    assert_eq!(row.state, SubagentState::Done);
    assert_eq!(row.done_evidence, Some(DoneEvidence::Lifecycle));
    assert_eq!(row.final_report.as_deref(), Some("all done"));
}

#[test]
fn long_report_is_cut_at_the_default_and_says_so() {
    let work = TempDir::new().unwrap();
    let dir = seed(work.path(), "unit-a", 1);
    with_report(&dir, &"x".repeat(20_000), true);
    append(&dir, vec![terminal(Outcome::Succeeded, "done"), released()]);
    let report = rows(work.path()).remove(0).final_report.unwrap();
    assert!(report.contains("truncated at 8192 bytes"));
    assert!(report.len() < 8_500);
}

#[test]
fn unverified_worker_is_forward_unknown() {
    let work = TempDir::new().unwrap();
    let dir = seed(work.path(), "unit-a", 1);
    append(
        &dir,
        vec![
            terminal(Outcome::Succeeded, "done"),
            NativeEvent::OwnershipUnverified {
                detail: "missing".into(),
            },
        ],
    );
    let row = rows(work.path()).remove(0);
    assert_eq!(row.state, SubagentState::ForwardUnknown);
    assert!(row
        .terminal_reason
        .unwrap()
        .contains("could not be proven stopped"));
}

#[test]
fn failed_and_cancelled_rows_carry_the_reason() {
    let work = TempDir::new().unwrap();
    let failed = seed(work.path(), "failed", 1);
    let cancelled = seed(work.path(), "cancelled", 2);
    append(&failed, vec![terminal(Outcome::Failed, "boom"), released()]);
    append(
        &cancelled,
        vec![terminal(Outcome::Cancelled, "stop"), released()],
    );
    let rows = rows(work.path());
    let failed = rows
        .iter()
        .find(|row| row.agent_id == "codex:failed")
        .unwrap();
    let cancelled = rows
        .iter()
        .find(|row| row.agent_id == "codex:cancelled")
        .unwrap();
    assert_eq!(
        (failed.state, failed.terminal_reason.as_deref()),
        (SubagentState::Failed, Some("boom"))
    );
    assert_eq!(
        (cancelled.state, cancelled.terminal_reason.as_deref()),
        (SubagentState::Cancelled, Some("stop"))
    );
}

#[test]
fn only_the_newest_invocation_of_a_unit_is_listed() {
    let work = TempDir::new().unwrap();
    let old = seed(work.path(), "unit-a", 1);
    append(&old, vec![terminal(Outcome::Succeeded, "done"), released()]);
    seed(work.path(), "unit-a", 2);
    let rows = rows(work.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, SubagentState::ForwardWait);
}

#[test]
fn corrupt_record_is_skipped_not_fatal() {
    let work = TempDir::new().unwrap();
    let corrupt = seed(work.path(), "bad", 1);
    seed(work.path(), "good", 2);
    std::fs::write(corrupt.join("events.jsonl"), b"{\"seq\":1").unwrap();
    let rows = rows(work.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].agent_id, "codex:good");
}

#[test]
fn json_row_has_the_ordinary_shape() {
    let work = TempDir::new().unwrap();
    seed(work.path(), "unit-a", 1);
    let value = serde_json::to_value(&rows(work.path())[0]).unwrap();
    assert_eq!(value["agent_id"], "codex:unit-a");
    assert_eq!(value["state"], "forward-wait");
    assert!(value.get("forward").is_none());
}

#[test]
fn incomplete_report_is_marked_and_held_terminal_is_not_harvestable() {
    let work = TempDir::new().unwrap();
    let dir = seed(work.path(), "unit-a", 1);
    with_report(&dir, "partial", false);
    append(&dir, vec![terminal(Outcome::Failed, "failed")]);
    let row = rows(work.path()).remove(0);
    assert_eq!(row.state, SubagentState::ForwardWait);
    assert_eq!(row.final_report, None);
    append(&dir, vec![released()]);
    let row = rows(work.path()).remove(0);
    assert_eq!(
        row.final_report.as_deref(),
        Some("partial\n[incomplete: the worker ended without a final answer]")
    );
}
