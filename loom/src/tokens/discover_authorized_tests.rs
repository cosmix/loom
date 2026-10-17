use super::{discover_with, Source};
use crate::codex_lifecycle::{
    test_support::{Fixture, INV_A, STAGE},
    threads_with_state_root,
};
use crate::fs::session_files::save_session;
use crate::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use crate::tokens::{accumulate::AgentState, flow_build::build_flow, tests_fixture::codex_count};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

const NOW: i64 = 1_788_523_200;

fn job_and_rollout(
    fx: &Fixture,
    root: &Path,
    unit: &str,
    thread: &str,
    fresh: u64,
    status: &str,
) -> serde_json::Value {
    let row = fx.authorization_value(unit, INV_A);
    let authorization = fx.authorization(unit, INV_A).unwrap();
    let path = fx.write_job(&authorization, unit, status).unwrap();
    let mut job: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    job["threadId"] = json!(thread);
    fs::write(path, serde_json::to_vec(&job).unwrap()).unwrap();
    let context = json!({"type": "turn_context", "payload": {
        "model": "gpt-5.6-terra", "effort": "xhigh"}});
    fs::write(
        root.join(format!("rollout-2026-09-04T12-00-00-{thread}.jsonl")),
        format!("{context}\n{}\n", codex_count(NOW, [fresh, 0, 0])),
    )
    .unwrap();
    row
}

fn receipt(fx: &Fixture, thread: &str) {
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
        backend_id: thread.into(),
        state: ForwardState::Succeeded,
        observed_at: chrono::Utc::now(),
        exit_code: Some(0),
        codex_thread_id: Some(thread.into()),
        locator: None,
        model: None,
        effort: None,
    };
    fs::write(
        receipts_path(&fx.work_dir, STAGE).unwrap(),
        format!("{}\n", receipt.encode_line().unwrap()),
    )
    .unwrap();
}

fn flow(sources: &[Source]) -> crate::tokens::TokenFlow {
    let mut stages = BTreeMap::new();
    for source in sources {
        let mut state = AgentState::new(source.kind, &source.path);
        for line in fs::read_to_string(&source.path).unwrap().lines() {
            state.ingest(line);
        }
        stages
            .entry(source.stage_id.clone())
            .or_insert_with(BTreeMap::new)
            .insert(source.key.clone(), state);
    }
    build_flow(&stages, NOW)
}

#[test]
fn authorized_jobs_without_receipts_or_claude_transcripts_reach_all_totals_once() {
    let fx = Fixture::new().unwrap();
    save_session(&fx.session, &fx.work_dir).unwrap();
    let root = fx.workspace.join("rollouts");
    fs::create_dir(&root).unwrap();
    let rows = [
        job_and_rollout(&fx, &root, "live", "thread-live", 45_773, "running"),
        job_and_rollout(&fx, &root, "done", "thread-done", 53_048, "completed"),
    ];
    fx.write_authorizations(&rows).unwrap();
    let discover = || {
        discover_with(
            &fx.work_dir,
            Some(&root),
            &mut BTreeMap::new(),
            NOW,
            |work, sessions| threads_with_state_root(work, sessions, &fx.state_root),
        )
        .unwrap()
    };
    let sources = discover();
    assert_eq!(sources.len(), 2);
    let totals = flow(&sources);
    assert_eq!(totals.plan.total.fresh(), 98_821);
    assert_eq!(totals.stages[0].total.fresh(), 98_821);
    assert_eq!(totals.stages[0].agents.len(), 2);
    assert_eq!(totals.models.len(), 1);
    assert_eq!(totals.models[0].model, "gpt-5.6-terra");
    assert_eq!(totals.models[0].effort.as_deref(), Some("xhigh"));
    assert_eq!(totals.models[0].total.fresh(), 98_821);
    receipt(&fx, "thread-done");
    let mut with_receipt = discover();
    let mut sources = sources;
    with_receipt.sort_by(|a, b| a.key.cmp(&b.key));
    sources.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(
        with_receipt, sources,
        "receipt and job name the same rollout once"
    );
}
