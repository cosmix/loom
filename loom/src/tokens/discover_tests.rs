use super::{discover, rollout_thread_id, Source, ThreadLookup, MISS_RETRY_SECS};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use crate::models::session::Session;
use crate::tokens::model::AgentKind;
use crate::tokens::tests_fixture::{append, Fixture, SESSION_UUID, THREAD_ID};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const NOW: i64 = 1_788_523_200;
const AGENT: &str = "a04baa183f888b773";

fn sources(fx: &Fixture, threads: &mut BTreeMap<String, ThreadLookup>, now: i64) -> Vec<Source> {
    discover(&fx.work_root, Some(&fx.codex_sessions()), threads, now).expect("discover")
}

fn keys(sources: &[Source]) -> Vec<(&str, &str)> {
    sources
        .iter()
        .map(|s| (s.stage_id.as_str(), s.key.as_str()))
        .collect()
}

/// A forward receipt for `THREAD_ID` in `stage`'s receipts file.
fn add_receipt(fx: &Fixture, stage: &str) {
    add_receipt_for(fx, stage, THREAD_ID);
}

/// A forward receipt for `thread_id` in `stage`'s receipts file.
fn add_receipt_for(fx: &Fixture, stage: &str, thread_id: &str) {
    let identity = ForwardIdentity::new(SESSION_UUID, "a0c0dec0ffee", "toolu_01", stage, "s-1")
        .expect("forward identity");
    let observation = ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id.clone(),
        agent_id: identity.agent_id.clone(),
        tool_use_id: identity.tool_use_id.clone(),
        stage_id: identity.stage_id.clone(),
        loom_session_id: identity.loom_session_id.clone(),
        backend: ForwardBackend::Direct,
        backend_id: thread_id.to_string(),
        state: ForwardState::Succeeded,
        observed_at: chrono::Utc::now(),
        exit_code: Some(0),
        codex_thread_id: Some(thread_id.to_string()),
        locator: None,
        model: Some("gpt-6.1-sol".to_string()),
        effort: None,
    };
    let path = receipts_path(&fx.work_root, stage).expect("receipts path");
    fs::create_dir_all(path.parent().expect("parent")).expect("receipts dir");
    let line = observation.encode_line().expect("encode receipt");
    append(&path, &format!("{line}\n"));
}

#[test]
fn work_dir_resolves_back_to_the_work_root() {
    let fx = Fixture::new();

    let work_dir = WorkDir::new(&fx.work_root).expect("work dir");

    assert_eq!(work_dir.root(), fx.work_root);
}

#[test]
fn every_session_of_a_handed_off_stage_counts() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    fx.add_session("alpha", "02ed9f4c-608e-4e2c-8edf-05d26adcd4d6", "");

    let found = sources(&fx, &mut BTreeMap::new(), NOW);

    let mut found = keys(&found);
    found.sort();
    let expected = [
        ("alpha", "main-01ed9f4c-608e-4e2c-8edf-05d26adcd4d6"),
        ("alpha", "main-02ed9f4c-608e-4e2c-8edf-05d26adcd4d6"),
    ];
    assert_eq!(found, expected);
}

#[test]
fn subagents_are_listed_and_unsafe_names_skipped() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    fx.add_subagent(SESSION_UUID, &format!("agent-{AGENT}.jsonl"), "");
    fx.add_subagent(SESSION_UUID, "agent-.hidden.jsonl", "");
    fx.add_subagent(SESSION_UUID, "agent-a b.jsonl", "");
    let row = json!({"agent_id": AGENT, "agent_type": "loom-software-engineer",
        "stage_id": "alpha", "parent_session_id": SESSION_UUID,
        "loom_session_id": "session-1", "ts": "2026-10-14T08:00:00.000Z"});
    let ledger = fx.work_root.join("subagents").join("alpha");
    fs::create_dir_all(&ledger).expect("ledger dir");
    append(&ledger.join("starts.jsonl"), &format!("{row}\n"));

    let found = sources(&fx, &mut BTreeMap::new(), NOW);

    let subagents: Vec<_> = found
        .iter()
        .filter(|s| s.kind == AgentKind::Subagent)
        .collect();
    assert_eq!(subagents.len(), 1, "{found:?}");
    assert_eq!(subagents[0].key, AGENT);
    assert_eq!(
        subagents[0].agent_type.as_deref(),
        Some("loom-software-engineer")
    );
}

#[test]
fn sessions_without_transcript_or_safe_stage_are_skipped() {
    let fx = Fixture::new();
    fx.add_session("../escape", SESSION_UUID, "");
    let mut bare = Session::new();
    bare.stage_id = Some("beta".into());
    save_session(&bare, &fx.work_root).expect("save session");

    assert!(sources(&fx, &mut BTreeMap::new(), NOW).is_empty());
}

#[test]
fn one_transcript_named_twice_is_one_source() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    fx.add_session("alpha", SESSION_UUID, "");

    assert_eq!(sources(&fx, &mut BTreeMap::new(), NOW).len(), 1);
}

#[test]
fn missed_rollout_is_looked_for_again_only_after_the_retry_delay() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    add_receipt(&fx, "alpha");
    let mut threads = BTreeMap::new();
    assert_eq!(sources(&fx, &mut threads, NOW).len(), 1);
    assert_eq!(threads[THREAD_ID].rollout, None);

    let dir = fx.codex_sessions().join("2026/10/14");
    fs::create_dir_all(&dir).expect("rollout dir");
    let rollout = dir.join(format!("rollout-2026-10-14T10-58-07-{THREAD_ID}.jsonl"));
    fs::write(&rollout, "").expect("rollout");
    assert_eq!(
        sources(&fx, &mut threads, NOW + 60).len(),
        1,
        "walked again"
    );

    let found = sources(&fx, &mut threads, NOW + MISS_RETRY_SECS);
    let codex: Vec<_> = found
        .iter()
        .filter(|s| s.kind == AgentKind::Codex)
        .collect();
    assert_eq!(codex.len(), 1);
    assert_eq!(codex[0].key, format!("codex-{THREAD_ID}"));
    assert_eq!(codex[0].path, rollout.to_str().expect("utf-8"));
}

#[test]
fn a_planted_checked_at_cannot_overflow_the_retry_age() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    add_receipt(&fx, "alpha");
    let dir = fx.codex_sessions().join("2026/10/14");
    fs::create_dir_all(&dir).expect("rollout dir");
    let rollout = dir.join(format!("rollout-2026-10-14T10-58-07-{THREAD_ID}.jsonl"));
    fs::write(&rollout, "").expect("rollout");

    for checked_at in [i64::MIN, i64::MAX] {
        let mut threads = BTreeMap::from([(
            THREAD_ID.to_owned(),
            ThreadLookup {
                rollout: None,
                checked_at,
            },
        )]);

        let found = sources(&fx, &mut threads, NOW);

        let looked_up = threads[THREAD_ID].rollout.is_some();
        assert_eq!(
            looked_up,
            checked_at == i64::MIN,
            "looked up again at {checked_at}"
        );
        assert_eq!(
            found.iter().any(|s| s.kind == AgentKind::Codex),
            looked_up,
            "codex source at {checked_at}"
        );
    }
}

#[test]
fn a_short_id_equal_to_the_last_segment_does_not_resolve_to_another_threads_rollout() {
    let fx = Fixture::new();
    fx.add_session("alpha", SESSION_UUID, "");
    let short = THREAD_ID.rsplit('-').next().expect("last segment");
    add_receipt_for(&fx, "alpha", short);
    let dir = fx.codex_sessions().join("2026/10/14");
    fs::create_dir_all(&dir).expect("rollout dir");
    let rollout = dir.join(format!("rollout-2026-10-14T10-58-07-{THREAD_ID}.jsonl"));
    fs::write(&rollout, "").expect("rollout");
    let mut threads = BTreeMap::new();

    let found = sources(&fx, &mut threads, NOW);

    assert!(
        found.iter().all(|s| s.kind != AgentKind::Codex),
        "{found:?}"
    );
    assert_eq!(threads[short].rollout, None);
}

#[test]
fn rollout_thread_id_requires_the_timestamped_rollout_name() {
    fn id(name: &str) -> Option<&str> {
        rollout_thread_id(Path::new(name))
    }

    let full = format!("rollout-2026-10-14T10-58-07-{THREAD_ID}.jsonl");
    assert_eq!(id(&full), Some(THREAD_ID));
    assert_eq!(id("rollout-2026-10-14T10-58-07-abc.jsonl"), Some("abc"));
    assert_eq!(id(&format!("rollout-{THREAD_ID}.jsonl")), None);
    assert_eq!(
        id(&format!("session-2026-10-14T10-58-07-{THREAD_ID}.jsonl")),
        None
    );
    assert_eq!(id("rollout-2026-10-14T10-58-07-abc.json"), None);
    assert_eq!(id("rollout-2026-10-14T10-58-07.jsonl"), None);
    assert_eq!(id("rollout-2026-10-14T10-58-07-.jsonl"), None);
}
