//! P1 (plan step A1), continued: the status a task notification carries for a background Bash
//! that failed, a background Bash stopped with TaskStop (or KillShell), and a background Agent
//! stopped with TaskStop, and the `background_tasks` of the Stop events around each stop.

use serde_json::Value;
use serial_test::serial;

use super::evidence::{backgrounded, joined, tool_result};
use super::p1::{
    await_notification, await_opened_turn, calls_since, note_agents, note_result, prompt_for,
    Wanted, SLEEPER,
};
use super::probe::Probe;
use crate::diagnose::turn_starts;

/// Accepts any tool input.
const ANY: fn(&Value) -> bool = |_| true;

/// The text between `<name>` and `</name>` in `line`.
fn tag<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.split_once(&format!("<{name}>"))?.1;
    Some(rest.split_once(&format!("</{name}>"))?.0)
}

/// Each notification line with its status and summary, for a note.
fn describe(found: &[String]) -> String {
    let each = found.iter().map(|line| {
        let status = tag(line, "status").unwrap_or("(no status tag)");
        let summary = tag(line, "summary").unwrap_or("(no summary tag)");
        format!("status: {status}\nsummary: {summary}\nnotification record:\n{line}")
    });
    joined(each)
}

/// The `field` of the `toolUseResult` in `call`'s result: the launch result's
/// `backgroundTaskId` or `agentId`.
fn launched(p: &Probe, call: &Value, field: &str) -> Option<String> {
    let records = p.records();
    let result = tool_result(&records, call["id"].as_str()?)?;
    let id = result.pointer(&format!("/toolUseResult/{field}"))?;
    id.as_str().map(str::to_owned)
}

/// The `background_tasks` of each Stop event in `spool[from..to]`, one line per event.
fn stop_lists(spool: &[Value], from: usize, to: usize) -> String {
    let range = spool.get(from..to).unwrap_or_default();
    let stops = range
        .iter()
        .enumerate()
        .filter(|(_, e)| e["event"] == "Stop");
    let lines = stops.map(|(n, e)| {
        format!(
            "spool line {}: {}",
            from + n + 1,
            e["payload"]["background_tasks"]
        )
    });
    joined(lines)
}

/// Asks for task `id` to be stopped with each of `tools` in turn, until one is called. A
/// plan session may lack the tool, as it lacks SendMessage: when none is called, notes the
/// model's reply as "unavailable" and returns `None`.
fn ask_stop(p: &mut Probe, id: &str, tools: &[&'static str]) -> Option<Value> {
    let mut reply = String::new();
    for tool in tools {
        let ask = format!(
            "Use the {tool} tool to stop the background task whose id is {id}. Then tell me \
             what the tool returned."
        );
        let before = p.records().len();
        p.run_turn(&ask, &format!("the {tool} stop request"));
        // TaskStop is a deferred tool: the model loads it with ToolSearch first, and the call
        // record can land after the turn wait returns, so give it a few seconds.
        let called = || calls_since(&p.records(), before, &[(tool, ANY)]).is_some();
        p.within(super::secs(15), called);
        if let Some(calls) = calls_since(&p.records(), before, &[(tool, ANY)]) {
            return calls.into_iter().next();
        }
        reply = joined(p.lines().iter().skip(before));
    }
    let body = format!("none of {tools:?} was called; records after the last ask:\n{reply}");
    p.out.note("Stop tool unavailable", &body);
    None
}

/// Stops task `id` and notes the evidence: the Stop events' `background_tasks` before and
/// after, the stop call and its result, and the notification. Returns whether a tool stopped it.
fn stop_and_note(p: &mut Probe, what: &str, id: &str, tools: &[&'static str]) -> bool {
    let (marked, lines) = (p.spool().len(), p.lines().len());
    let before = stop_lists(&p.spool(), 0, marked);
    p.out.note(
        &format!("{what}: Stop events before the stop request"),
        &before,
    );
    let Some(call) = ask_stop(p, id, tools) else {
        return false;
    };
    note_result(p, &format!("{what}: stop call and result"), &call);
    let starts = turn_starts(p.r(), &p.id);
    let found = await_notification(p, lines, 20);
    await_opened_turn(p, starts, 10, "the stop notification's turn");
    let body = if found.is_empty() {
        "no task notification within 20 s of the stop".to_owned()
    } else {
        describe(&found)
    };
    p.out
        .note(&format!("{what}: task notification after the stop"), &body);
    let spool = p.spool();
    let after = stop_lists(&spool, marked, spool.len());
    p.out.note(
        &format!("{what}: Stop events after the stop request"),
        &after,
    );
    true
}

/// Scenario 1: a background Bash that exits 7.
fn failed_bash(p: &mut Probe) {
    let ask = "Start the shell command `sleep 3; exit 7` with the Bash tool with \
               run_in_background set to true, then, without waiting for it, reply with the \
               single word STARTED.";
    let lines = p.lines().len();
    let wanted: [Wanted; 1] = [("Bash", backgrounded)];
    let calls = prompt_for(p, ask, "the failing background Bash launch", &wanted);
    let starts = turn_starts(p.r(), &p.id);
    note_result(p, "Failing Bash launch result", &calls[0]);
    let found = await_notification(p, lines, 60);
    if found.is_empty() {
        p.miss("no task notification for the failing background Bash within 60 s");
    }
    await_opened_turn(p, starts, 10, "the notification's turn");
    p.out.note(
        "Failed background Bash: task notification",
        &describe(&found),
    );
}

/// Scenario 2: a background Bash stopped by the model; whether a stop tool exists.
fn stopped_bash(p: &mut Probe) -> bool {
    let ask = "Start the shell command `sleep 60; echo LATE` with the Bash tool with \
               run_in_background set to true, then, without waiting for it, reply with the \
               single word STARTED.";
    let wanted: [Wanted; 1] = [("Bash", backgrounded)];
    let calls = prompt_for(p, ask, "the background Bash launch to stop", &wanted);
    note_result(p, "Bash to stop: launch result", &calls[0]);
    let Some(task) = launched(p, &calls[0], "backgroundTaskId") else {
        p.fail("the background Bash launch result names no backgroundTaskId");
    };
    stop_and_note(p, "Stopped Bash", &task, &["TaskStop", "KillShell"])
}

/// Scenario 3: a background Agent stopped by the model.
fn stopped_agent(p: &mut Probe) {
    let ask = format!(
        "Use the Agent tool with run_in_background set to true, subagent_type \
         \"general-purpose\", description \"probe background agent\" and this prompt: \
         \"{SLEEPER}\" Do not wait for it or check on it. Reply with the single word LAUNCHED."
    );
    let wanted: [Wanted; 1] = [("Agent", backgrounded)];
    let calls = prompt_for(p, &ask, "the background Agent launch to stop", &wanted);
    note_result(p, "Agent to stop: launch result", &calls[0]);
    let Some(agent) = launched(p, &calls[0], "agentId") else {
        p.fail("the background Agent launch result names no agentId");
    };
    stop_and_note(p, "Stopped Agent", &agent, &["TaskStop"]);
    note_agents(p);
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_failed_and_stopped_task_statuses() {
    let mut p = Probe::open("p1-status", |_, _| {});
    failed_bash(&mut p);
    if stopped_bash(&mut p) {
        stopped_agent(&mut p);
    } else {
        p.out.note(
            "Stopped Agent",
            "skipped: no stop tool exists in this session",
        );
    }
    p.finish();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_notification_names_its_status_and_summary() {
        let line = r#"{"content":"<task-notification>\n<task-id>b1</task-id>\n<status>failed</status>\n<summary>Background command \"x\" failed with exit code 7</summary>\n</task-notification>"}"#;
        assert_eq!(tag(line, "status"), Some("failed"));
        assert!(tag(line, "summary").unwrap().ends_with("exit code 7"));
        assert_eq!(tag(line, "missing"), None);
    }

    #[test]
    fn stop_lists_hold_each_stop_events_tasks_in_range() {
        let stop = |tasks: Value| json!({"event": "Stop", "payload": {"background_tasks": tasks}});
        let spool = vec![
            stop(json!([{"id": "a"}])),
            json!({"event": "SubagentStop"}),
            stop(json!([])),
        ];
        assert_eq!(
            stop_lists(&spool, 0, 3),
            "spool line 1: [{\"id\":\"a\"}]\nspool line 3: []"
        );
        assert_eq!(stop_lists(&spool, 1, 3), "spool line 3: []");
        assert_eq!(stop_lists(&spool, 5, 9), "");
    }
}
