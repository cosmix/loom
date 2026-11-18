use std::path::Path;

use super::ensure_no_native_workers;
use crate::native_worker::model::{NativeEvent, NativeManifest, Outcome, NATIVE_DEADLINE_SECS};
use crate::native_worker::store::{create_invocation, invocation_dir, EventLog};

const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";

pub(crate) fn seed_active_worker(work_dir: &Path, stage_id: &str, unit: &str) {
    std::fs::create_dir_all(work_dir).unwrap();
    let manifest = NativeManifest {
        v: 1,
        stage_id: stage_id.into(),
        unit_id: unit.into(),
        invocation_id: INVOCATION.into(),
        owner_session_id: "session-1".into(),
        workspace_root: work_dir.to_path_buf(),
        brief_path: work_dir.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: chrono::Utc::now(),
    };
    create_invocation(work_dir, &manifest, b"brief").unwrap();
}

fn terminal_log(work_dir: &Path) -> EventLog {
    let mut log = EventLog::open(&invocation_dir(work_dir, "s1", INVOCATION)).unwrap();
    log.append(NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "done".into(),
    })
    .unwrap();
    log
}

#[test]
fn gate_ok_without_records_and_after_release() {
    let temp = tempfile::TempDir::new().unwrap();
    let work = temp.path();
    assert!(ensure_no_native_workers(work, "s1").is_ok());
    seed_active_worker(work, "s1", "unit");
    let error = ensure_no_native_workers(work, "s1").unwrap_err();
    assert!(format!("{error:#}").contains("running native Codex workers"));
    terminal_log(work)
        .append(NativeEvent::OwnershipReleased {
            by: "operator".into(),
        })
        .unwrap();
    assert!(ensure_no_native_workers(work, "s1").is_ok());
}

#[test]
fn gate_names_release_command_when_unverified() {
    let temp = tempfile::TempDir::new().unwrap();
    let work = temp.path();
    seed_active_worker(work, "s1", "unit");
    terminal_log(work)
        .append(NativeEvent::OwnershipUnverified {
            detail: "unknown".into(),
        })
        .unwrap();
    let error = ensure_no_native_workers(work, "s1").unwrap_err();
    assert!(format!("{error:#}").contains("loom subagents release --stage"));
}
