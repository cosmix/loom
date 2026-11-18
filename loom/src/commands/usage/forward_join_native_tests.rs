use super::*;
use crate::native_worker::{
    model::{NativeEvent, NativeManifest},
    store::{create_invocation, EventLog},
};
use sha2::{Digest, Sha256};

fn seed(work: &Path, stage: &str, started: bool) -> std::path::PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: stage.into(),
        unit_id: "unit-1".into(),
        invocation_id: "inv-0123456789abcdef0123456789abcdef".into(),
        owner_session_id: "session-1".into(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: hex::encode(Sha256::digest(b"brief")),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: 1500,
        created_at: chrono::Utc::now(),
    };
    let dir = create_invocation(work, &manifest, b"brief").unwrap();
    if started {
        EventLog::open(&dir)
            .unwrap()
            .append(NativeEvent::ThreadStarted {
                thread_id: "thread-n".into(),
            })
            .unwrap();
    }
    dir
}

#[test]
fn corrupt_native_record_makes_the_scope_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let dir = seed(root.path(), "stage-a", true);
    assert!(ForwardJoin::load(Some(root.path())).scope_available());
    std::fs::write(dir.join("events.jsonl"), b"{\"seq\":1").unwrap();
    assert!(!ForwardJoin::load(Some(root.path())).scope_available());
}

#[test]
fn native_thread_joins_the_codex_scope_threads() {
    let root = tempfile::tempdir().unwrap();
    seed(root.path(), "stage-a", true);
    let expected = crate::native_worker::index::stage_threads(root.path(), "stage-a");
    let join = ForwardJoin::load(Some(root.path()));
    let rows = join.receipt_threads();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread_id.as_deref(), Some("thread-n"));
    assert_eq!(rows[0].stage_id, "stage-a");
    assert_eq!(rows[0].observed_at, expected[0].started_at);
}

#[test]
fn root_without_native_workers_adds_nothing() {
    let root = tempfile::tempdir().unwrap();
    assert!(ForwardJoin::load(Some(root.path()))
        .receipt_threads()
        .is_empty());
    assert!(ForwardJoin::load(None).receipt_threads().is_empty());
}
