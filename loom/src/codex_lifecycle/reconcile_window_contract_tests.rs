//! Contract for the bounded codex reconcile: historical session records
//! cannot crowd live stages out of the scan, and a stage past the per-poll
//! window is reached on the next poll through the returned cursor.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::Utc;

use crate::codex_lifecycle::reconcile_from;
use crate::models::session::{Session, SessionStatus};

fn session(id: String, stage_id: String, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id);
    session.id = id;
    session.status = status;
    session
}

fn write_unreadable_ledger(work_dir: &Path, stage_id: &str) {
    let dir = work_dir.join("subagents").join(stage_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("codex.jsonl"), "not json\n").unwrap();
}

/// 256 Completed records precede 65 Running sessions of 65 stages; two polls
/// together reconcile every one of the 65 stages.
#[test]
fn every_active_stage_is_reconciled_within_two_polls() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join("work");
    let state_root = temp.path().join("state");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::create_dir_all(&state_root).unwrap();
    let mut sessions: Vec<Session> = (0..256)
        .map(|n| {
            let id = format!("old-{n:03}");
            session(id.clone(), id, SessionStatus::Completed)
        })
        .collect();
    let mut expected = BTreeSet::new();
    for n in 0..65 {
        let stage_id = format!("s{n:03}");
        write_unreadable_ledger(&work_dir, &stage_id);
        sessions.push(session(
            format!("sess-{n:03}"),
            stage_id.clone(),
            SessionStatus::Running,
        ));
        expected.insert(stage_id);
    }

    let first = reconcile_from(&work_dir, &sessions, &state_root, Utc::now(), 0).unwrap();
    let second = reconcile_from(
        &work_dir,
        &sessions,
        &state_root,
        Utc::now(),
        first.next_cursor,
    )
    .unwrap();

    let seen: BTreeSet<String> = first
        .entries
        .iter()
        .chain(&second.entries)
        .map(|entry| entry.stage_id.clone())
        .collect();
    let missing: Vec<&String> = expected.difference(&seen).collect();
    assert!(missing.is_empty(), "never reconciled: {missing:?}");
}
