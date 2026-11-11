//! Contract: the planner token pass keeps its read offsets in the checkpoint, so a pass on a new
//! thread, with a fresh memo, gives the same totals instead of reading the transcripts again.

use std::collections::BTreeMap;
use std::fs;
use std::thread;

use serde_json::Value;

use super::{planner_pass, AgentFlow, PlannerGroup, TokenUsage, CHECKPOINT_FILE};

const MAIN: &str =
    include_str!("../../tests/fixtures/plan_session/transcripts/subagent-main.jsonl");
const AGENT: &str =
    include_str!("../../tests/fixtures/plan_session/transcripts/subagent-agent.jsonl");

/// Each row's total in group `s1`, by agent id.
fn totals(flows: &[(String, Vec<AgentFlow>)]) -> BTreeMap<String, TokenUsage> {
    let group = flows.iter().find(|(id, _)| id == "s1");
    let rows = group.map(|(_, rows)| rows.as_slice()).unwrap_or_default();
    rows.iter()
        .map(|flow| (flow.id.clone(), flow.total))
        .collect()
}

#[test]
fn a_restarted_planner_pass_does_not_double_count() {
    let tmp = tempfile::tempdir().unwrap();
    let tokens = tmp.path().join("tokens");
    fs::create_dir_all(&tokens).unwrap();
    let main = tmp.path().join("s1.jsonl");
    let agent = tmp.path().join("s1/subagents/agent-a7f3.jsonl");
    fs::create_dir_all(agent.parent().unwrap()).unwrap();
    fs::write(&main, MAIN).unwrap();
    fs::write(&agent, AGENT).unwrap();
    let subagents = vec![("a7f3".to_owned(), agent, Some("Explore".to_owned()))];
    let groups = vec![PlannerGroup {
        id: "s1".to_owned(),
        main,
        subagents,
    }];
    let now = chrono::Utc::now().timestamp();

    let first = totals(&planner_pass(&tokens, &groups, now).unwrap());
    let a7f3 = first.get("a7f3").copied().unwrap_or_default();
    assert!(a7f3.fresh() > 0, "no a7f3 row with usage: {first:?}");

    let dir = tokens.clone();
    // A new thread has a fresh thread-local memo: only the checkpoint on disk carries state.
    let pass = thread::spawn(move || planner_pass(&dir, &groups, now + 5).unwrap());
    let flows: Vec<(String, Vec<AgentFlow>)> = pass.join().expect("the restarted pass panicked");
    let restarted = totals(&flows);
    assert_eq!(restarted, first, "the restarted pass counted again");

    let saved = fs::read(tokens.join(CHECKPOINT_FILE)).expect("checkpoint.json");
    let checkpoint: Value = serde_json::from_slice(&saved).unwrap();
    let group = &checkpoint["stages"]["s1"];
    assert!(
        group.is_object(),
        "no s1 group in the checkpoint: {checkpoint}"
    );
}
