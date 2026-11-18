use super::*;
use crate::codex_lifecycle::test_support::{Fixture, STAGE};
use crate::fs::session_files::save_session;
use crate::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use crate::native_worker::{
    model::{NativeEvent, NativeManifest},
    store::{create_invocation, EventLog},
};
use sha2::{Digest, Sha256};
use std::fs;

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

fn unit() -> NativeUnit {
    NativeUnit {
        unit_id: "unit-1".into(),
        effort: "xhigh".into(),
    }
}

const NOW: i64 = 1_788_523_200;

fn setup(started: bool) -> (Fixture, PathBuf) {
    let fx = Fixture::new().unwrap();
    save_session(&fx.session, &fx.work_dir).unwrap();
    let root = fx.workspace.join("rollouts");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("rollout-2026-09-04T12-00-00-thread-n.jsonl"),
        "{}\n",
    )
    .unwrap();
    seed(&fx.work_dir, STAGE, started);
    (fx, root)
}

fn discover_fixture(fx: &Fixture, root: &Path) -> Vec<Source> {
    discover_with(
        &fx.work_dir,
        Some(root),
        &mut BTreeMap::new(),
        NOW,
        |_, _| Vec::new(),
    )
    .unwrap()
}

fn receipt(fx: &Fixture) {
    let identity = ForwardIdentity::new("parent", "agent", "tool", STAGE, &fx.session.id).unwrap();
    let receipt = ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id,
        agent_id: identity.agent_id,
        tool_use_id: identity.tool_use_id,
        stage_id: identity.stage_id,
        loom_session_id: identity.loom_session_id,
        backend: ForwardBackend::Direct,
        backend_id: "thread-n".into(),
        state: ForwardState::Succeeded,
        observed_at: chrono::Utc::now(),
        exit_code: Some(0),
        codex_thread_id: Some("thread-n".into()),
        locator: None,
        model: None,
        effort: None,
    };
    let path = receipts_path(&fx.work_dir, STAGE).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("{}\n", receipt.encode_line().unwrap())).unwrap();
}

#[test]
fn native_thread_is_attributed_to_its_stage() {
    let (fx, root) = setup(true);
    let sources = discover_fixture(&fx, &root);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].key, "codex-thread-n");
    assert_eq!(sources[0].stage_id, STAGE);
    assert_eq!(sources[0].forwarder, None);
    assert_eq!(sources[0].unit, Some(unit()));
}

#[test]
fn native_thread_named_by_a_receipt_is_counted_once() {
    let (fx, root) = setup(true);
    receipt(&fx);
    let sources = discover_fixture(&fx, &root);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].key, "codex-thread-n");
    assert_eq!(sources[0].stage_id, STAGE);
    assert_eq!(sources[0].forwarder.as_deref(), Some("agent"));
    assert_eq!(sources[0].unit, Some(unit()));
}

#[test]
fn unit_without_a_thread_adds_no_source() {
    let (fx, root) = setup(false);
    assert!(discover_fixture(&fx, &root).is_empty());
}
