use super::*;
use crate::native_worker::{
    model::{NativeEvent, NativeManifest},
    store::{create_invocation, EventLog},
};
use sha2::{Digest, Sha256};

fn seed(work: &Path, stage: &str, started: bool) {
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
}

fn fixture(started: bool) -> (tempfile::TempDir, WorkDir) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".loom/work");
    std::fs::create_dir_all(&root).unwrap();
    let work = WorkDir::new(&root).unwrap();
    seed(work.root(), "s1", started);
    (temp, work)
}

#[test]
fn native_model_is_listed_as_requested() {
    let (_temp, work) = fixture(true);
    assert_eq!(
        execution_models_for_stage(&work, "s1"),
        ["gpt-6.1-terra (requested)"]
    );
}

#[test]
fn native_model_and_codex_row_collapse() {
    let (_temp, work) = fixture(true);
    let stage = work.root().join("subagents/s1");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("codex.jsonl"), "{\"model\":\"gpt-6.1-terra\"}\n").unwrap();
    assert_eq!(
        execution_models_for_stage(&work, "s1"),
        ["gpt-6.1-terra (requested)"]
    );
}

#[test]
fn worker_without_a_thread_lists_no_model() {
    let (_temp, work) = fixture(false);
    assert!(execution_models_for_stage(&work, "s1").is_empty());
}
