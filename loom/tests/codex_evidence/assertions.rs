use anyhow::{ensure, Context, Result};
use loom::subagent_lifecycle::WorkerOutcome;
use serde_json::Value;
use std::process::Output;

use crate::fixture::{Fixture, Forwarder, Launch, EFFORT, LOOM_SESSION, MODEL, STAGE};

pub fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_wrapper_timed_out(fixture: &Fixture, launch: &Launch) -> Result<()> {
    assert_exit(&launch.wrapper, 124);
    let stdout = String::from_utf8_lossy(&launch.wrapper.stdout);
    ensure!(
        stdout.contains("LOOM-FORWARD-START"),
        "wrapper omitted START"
    );
    ensure!(
        stdout.contains(&format!(
            "LOOM-FORWARD-END {{\"v\":1,\"backend\":\"companion\",\"job_id\":\"{}\",\"outcome\":\"timed_out\",\"exit_code\":124}}",
            launch.job_id
        )),
        "wrapper omitted the timed-out END line"
    );
    ensure!(
        stdout.contains("state: timed_out"),
        "wrapper omitted timed-out evidence"
    );
    ensure!(
        stdout.contains("exit: 124"),
        "wrapper omitted exit 124 evidence"
    );
    assert_call_contract(fixture, launch, &["task", "status", "cancel"])
}

pub fn assert_wrapper_terminal(fixture: &Fixture, launch: &Launch, status: &str) -> Result<()> {
    let expected_exit = if status == "completed" { 0 } else { 1 };
    assert_exit(&launch.wrapper, expected_exit);
    let stdout = String::from_utf8_lossy(&launch.wrapper.stdout);
    let marker = if status == "cancelled" {
        "canceled"
    } else if status == "completed" {
        "succeeded"
    } else {
        status
    };
    let evidence_state = if status == "completed" {
        "succeeded"
    } else {
        status
    };
    ensure!(
        stdout.contains("LOOM-FORWARD-START"),
        "wrapper omitted START"
    );
    ensure!(
        stdout.contains("LOOM-FORWARD-END"),
        "terminal wrapper omitted END"
    );
    ensure!(
        stdout.contains(&format!("\"outcome\":\"{marker}\"")),
        "wrong terminal marker"
    );
    ensure!(
        stdout.contains(&format!("state: {evidence_state}")),
        "wrong evidence state"
    );
    assert_call_contract(fixture, launch, &["task", "status", "result"])
}

pub fn assert_call_contract(
    fixture: &Fixture,
    launch: &Launch,
    expected_commands: &[&str],
) -> Result<()> {
    let calls = fixture.calls(launch)?;
    let commands: Vec<_> = calls
        .iter()
        .filter_map(|call| call["command"].as_str())
        .collect();
    ensure!(
        commands == expected_commands,
        "unexpected companion calls: {calls:?}"
    );
    let session_id = launch.authorization.encoded_session_id();
    let plugin_data = fixture
        .home
        .join(".codex/plugin-data")
        .display()
        .to_string();
    for call in &calls {
        ensure!(
            call["sessionId"].as_str() == Some(session_id.as_str()),
            "companion session identity changed"
        );
        ensure!(
            call["pluginData"].as_str() == Some(plugin_data.as_str()),
            "companion state root changed"
        );
    }
    assert_task_args(&calls[0])?;
    assert_status_args(&calls[1], &launch.job_id)?;
    if expected_commands.get(2) == Some(&"cancel") {
        assert_cancel_args(&calls[2], &launch.job_id)?;
    }
    Ok(())
}

fn assert_task_args(call: &Value) -> Result<()> {
    let args = string_args(call)?;
    for required in [
        "--background",
        "--json",
        "--write",
        "--model",
        MODEL,
        "--effort",
        EFFORT,
    ] {
        ensure!(args.contains(&required), "task call omitted {required}");
    }
    Ok(())
}

fn assert_status_args(call: &Value, job_id: &str) -> Result<()> {
    let args = string_args(call)?;
    ensure!(
        args == [job_id, "--wait", "--json", "--timeout-ms", "540000"],
        "status call was not the one bounded exact wait: {args:?}"
    );
    Ok(())
}

fn assert_cancel_args(call: &Value, job_id: &str) -> Result<()> {
    let args = string_args(call)?;
    ensure!(
        args == [job_id, "--json"],
        "cancel call was not the bounded cancel: {args:?}"
    );
    Ok(())
}

fn string_args(call: &Value) -> Result<Vec<&str>> {
    call["args"]
        .as_array()
        .context("fake call args are not an array")?
        .iter()
        .map(|value| value.as_str().context("fake call arg is not a string"))
        .collect()
}

pub fn assert_state(list: &Value, agent: &str, expected: &str) -> Result<()> {
    let rows = list.as_array().context("subagents list is not an array")?;
    let row = rows
        .iter()
        .find(|row| row["agent_id"] == agent)
        .with_context(|| format!("missing summary for {agent}: {rows:?}"))?;
    ensure!(row["state"] == expected, "wrong state for {agent}: {row}");
    if expected != "done" {
        ensure!(
            row.get("done_evidence").is_none(),
            "non-done row has done evidence"
        );
    }
    Ok(())
}

pub fn assert_unknown_after_poll(
    fixture: &Fixture,
    launch: &Launch,
    forwarder: &Forwarder,
) -> Result<()> {
    fixture.poll()?;
    ensure!(
        matches!(fixture.companion_outcome(launch), WorkerOutcome::Unknown(_)),
        "mismatched companion evidence was trusted"
    );
    ensure!(
        fixture.journal_values()?.is_empty(),
        "unknown evidence reached lifecycle journal"
    );
    let listed = fixture.list(forwarder)?;
    assert_state(&listed, &forwarder.agent, "forward-unknown")?;
    let harvest = fixture.harvest(forwarder)?;
    assert_exit(&harvest, 0);
    ensure!(
        String::from_utf8_lossy(&harvest.stdout).contains("nothing harvestable"),
        "unknown evidence was harvestable"
    );
    assert_exit(&fixture.watch(launch)?, 5);
    Ok(())
}

pub fn assert_lifecycle_identity(record: &Value, launch: &Launch) -> Result<()> {
    let identity = &record["identity"];
    ensure!(
        identity["kind"] == "codex",
        "lifecycle identity kind changed"
    );
    ensure!(identity["stage_id"] == STAGE, "lifecycle stage changed");
    ensure!(
        identity["loom_session_id"] == LOOM_SESSION,
        "lifecycle session changed"
    );
    ensure!(
        identity["parent_session_id"].as_str() == Some(launch.forwarder.parent.as_str()),
        "lifecycle parent changed"
    );
    ensure!(
        identity["forwarder_agent_id"].as_str() == Some(launch.forwarder.agent.as_str()),
        "lifecycle agent changed"
    );
    ensure!(
        identity["unit_id"].as_str() == Some(launch.authorization.unit_id.as_str()),
        "lifecycle unit changed"
    );
    ensure!(
        identity["invocation_id"].as_str() == Some(launch.authorization.invocation_id.as_str()),
        "lifecycle invocation changed"
    );
    let workspace = launch.authorization.workspace_root.display().to_string();
    ensure!(
        identity["workspace_root"].as_str() == Some(workspace.as_str()),
        "lifecycle workspace changed"
    );
    ensure!(
        identity["execution"]["mode"] == "companion",
        "lifecycle execution mode changed"
    );
    ensure!(
        identity["execution"]["job_id"].as_str() == Some(launch.job_id.as_str()),
        "lifecycle job changed"
    );
    Ok(())
}
