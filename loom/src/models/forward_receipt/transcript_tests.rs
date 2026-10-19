use std::fs;

use anyhow::Result;
use serde_json::json;

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;

const PARENT: &str = "parent-session";
const AGENT: &str = "agent-one";
const TOOL: &str = "tool-one";
const COMMAND: &str =
    "~/.claude/hooks/loom/codex-forward.sh task 'work' --model gpt-5.6-terra --effort xhigh --write";

fn identity() -> TranscriptIdentity {
    TranscriptIdentity {
        parent_session_id: PARENT.to_owned(),
        agent_id: AGENT.to_owned(),
    }
}

fn transcript(rows: &[serde_json::Value]) -> Result<Transcript> {
    let input = rows
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    decode(&input, identity())
}

fn tool_use(command: &str) -> serde_json::Value {
    json!({"type":"assistant","timestamp":"2026-09-13T10:00:00Z",
        "sessionId":PARENT,"agentId":AGENT,"message":{"content":[
            {"type":"tool_use","id":TOOL,"name":"Bash","input":{"command":command}}]}})
}

#[test]
fn bash_command_that_only_mentions_forwarder_is_not_an_invocation() -> Result<()> {
    let decoded = transcript(&[tool_use(r#"rg -n "codex-forward.sh task" doc/"#)])?;

    assert!(decoded.invocations.is_empty());
    Ok(())
}

#[test]
#[serial_test::serial]
fn duplicate_tool_use_id_is_unknown() -> Result<()> {
    let home = tempfile::tempdir()?;
    let _env = InstallRootsEnv::isolate(home.path());
    let decoded = transcript(&[tool_use(COMMAND), tool_use(COMMAND)])?;

    assert!(decoded.invocations.is_empty());
    Ok(())
}

/// `COMMAND` uses the `~` spelling, which counts only while the hooks
/// directory resolves to the default root: the environment is pinned there.
#[test]
#[serial_test::serial]
fn nested_task_output_path_is_resolved() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let _env = InstallRootsEnv::isolate(temp.path());
    let output = temp
        .path()
        .join("workspace-one")
        .join(PARENT)
        .join("tasks")
        .join("task-one.output");
    fs::create_dir_all(output.parent().expect("task output parent"))?;
    fs::write(
        &output,
        "LOOM-FORWARD-START {\"v\":1,\"backend\":\"direct\",\"thread_id\":\"thread-one\"}\n--- LOOM-FORWARD-OUTPUT ---\n",
    )?;
    let result = json!({"type":"user","timestamp":"2026-09-13T10:01:00Z",
        "sessionId":PARENT,"agentId":AGENT,
        "toolUseResult":{"backgroundTaskId":"task-one","nested":{"deeper":{"path":output}}},
        "message":{"content":[{"type":"tool_result","tool_use_id":TOOL,"content":""}]}});
    let decoded = transcript(&[tool_use(COMMAND), result])?;
    let result = decoded.invocations[0]
        .result
        .as_ref()
        .expect("paired tool result");

    assert!(matches!(
        evidence_channel(result, &[temp.path().to_path_buf()], PARENT),
        MarkerChannel::Started(_)
    ));
    Ok(())
}
