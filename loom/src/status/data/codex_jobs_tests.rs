use super::*;
use crate::native_worker::model::{NativeEvent, NativeManifest};
use crate::native_worker::store::{create_invocation, EventLog};
use chrono::TimeZone;
use sha2::{Digest, Sha256};

pub(in crate::status::data) fn seed(
    work: &std::path::Path,
    unit: &str,
    serial: u8,
    events: &[NativeEvent],
) {
    let manifest = NativeManifest {
        v: 1,
        stage_id: "s1".into(),
        unit_id: unit.into(),
        invocation_id: format!("inv-{serial:032x}"),
        owner_session_id: "session-1".into(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: hex::encode(Sha256::digest(b"brief")),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: 1500,
        created_at: Utc
            .timestamp_opt(1_700_000_000 + i64::from(serial), 0)
            .unwrap(),
    };
    let dir = create_invocation(work, &manifest, b"brief").unwrap();
    let mut log = EventLog::open(&dir).unwrap();
    for event in events {
        log.append(event.clone()).unwrap();
    }
}

pub(in crate::status::data) fn fixture() -> (tempfile::TempDir, WorkDir) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".loom/work");
    std::fs::create_dir_all(&root).unwrap();
    let work = WorkDir::new(&root).unwrap();
    (temp, work)
}

#[test]
fn a_stage_without_native_workers_has_no_jobs() {
    let (_temp, work) = fixture();
    assert!(codex_jobs_for_stage(&work, "s1").is_empty());
}

#[test]
fn jobs_carry_outcome_thread_and_end_time_and_serialize_to_the_contract() {
    let (_temp, work) = fixture();
    let thread = NativeEvent::ThreadStarted {
        thread_id: "thread-a".into(),
    };
    let done = NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "turn completed".into(),
    };
    let failed = NativeEvent::Terminal {
        outcome: Outcome::TimedOut,
        reason: "deadline".into(),
    };
    seed(work.root(), "hex-test", 1, &[thread, done]);
    seed(work.root(), "late", 2, &[failed]);
    let jobs = codex_jobs_for_stage(&work, "s1");
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].unit_id, "hex-test");
    assert_eq!(jobs[0].outcome, Some(Outcome::Succeeded));
    assert_eq!(jobs[0].thread_id.as_deref(), Some("thread-a"));
    assert!(jobs[0].ended_at.is_some());
    assert_eq!(jobs[1].thread_id, None);
    assert_eq!(jobs[1].outcome, Some(Outcome::TimedOut));

    let json = serde_json::to_value(&jobs[1]).unwrap();
    assert_eq!(json["unit_id"], "late");
    assert_eq!(json["model"], "gpt-6.1-terra");
    assert_eq!(json["effort"], "xhigh");
    assert_eq!(json["phase"], "terminal");
    assert_eq!(json["outcome"], "timed_out");
    assert_eq!(json["outcome_reason"], "deadline");
    assert_eq!(json["thread_id"], serde_json::Value::Null);
    assert_eq!(json["deadline_secs"], 1500);
    assert!(json["started_at"].as_str().unwrap().ends_with('Z'));
    assert!(json["ended_at"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn a_control_sequence_in_a_reason_is_flattened() {
    let mut job = CodexJob {
        unit_id: "u".into(),
        invocation_id: "inv".into(),
        model: "m".into(),
        effort: "high".into(),
        phase: Phase::Terminal,
        outcome: Some(Outcome::Failed),
        outcome_reason: Some("bad \x1b]52;c;x\x07 reason".into()),
        thread_id: None,
        started_at: Utc::now(),
        ended_at: None,
        deadline_secs: 1,
    };
    job.flatten();
    assert!(!job.outcome_reason.unwrap().contains(['\x1b', '\x07']));
}

#[test]
fn a_deadline_past_the_javascript_safe_range_is_clamped() {
    let mut job = CodexJob {
        unit_id: "u".into(),
        invocation_id: "inv".into(),
        model: "m".into(),
        effort: "high".into(),
        phase: Phase::Running,
        outcome: None,
        outcome_reason: None,
        thread_id: None,
        started_at: Utc::now(),
        ended_at: None,
        deadline_secs: u64::MAX,
    };
    job.flatten();
    assert_eq!(job.deadline_secs, MAX_SAFE_INTEGER);
    job.deadline_secs = 1500;
    job.flatten();
    assert_eq!(job.deadline_secs, 1500);
}
