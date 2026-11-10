//! P1 (plan step A1): background and foreground agents, a background Bash, both task
//! notifications, and a notification arriving while the main agent works and while it is idle.
//! Shared helpers for `p1_more`.

use std::fs;

use serde_json::Value;
use serial_test::serial;

use super::evidence::{
    backgrounded, containing, joined, last_stop_reason, lines_of, meta_of, parsed, spooled,
    subagent_files, tool_result, tool_uses,
};
use super::probe::Probe;
use super::secs;
use crate::diagnose::turn_starts;

/// What a probe's subagent does: long enough to overlap the main agent, one word of output.
pub const SLEEPER: &str =
    "Run the shell command `sleep 20` with the Bash tool, then reply with the single word DONE.";

/// The needle of a task-notification record.
pub const NOTIFICATION: &str = "task-notification";

pub fn foreground(input: &Value) -> bool {
    !backgrounded(input)
}

/// One `(tool, input test)` call the prompt asks for.
pub type Wanted = (&'static str, fn(&Value) -> bool);

/// The first new call per `wanted` entry in `records[before..]`, when every one was made.
pub fn calls_since(records: &[Value], before: usize, wanted: &[Wanted]) -> Option<Vec<Value>> {
    let new = records.get(before..).unwrap_or_default();
    let first = |(tool, keep): &Wanted| tool_uses(new, tool, keep).into_iter().next();
    wanted.iter().map(first).collect()
}

/// Sends `text` and waits for its turns; when Claude did not make every call `wanted` names,
/// sends it once more, then fails. Returns the calls.
pub fn prompt_for(p: &mut Probe, text: &str, what: &str, wanted: &[Wanted]) -> Vec<Value> {
    for attempt in 1..=2 {
        let before = p.records().len();
        p.run_turn(text, what);
        if let Some(calls) = calls_since(&p.records(), before, wanted) {
            return calls;
        }
        let names: Vec<&str> = wanted.iter().map(|(tool, _)| *tool).collect();
        let body = format!("attempt {attempt}: not every call of {names:?} was made as asked");
        p.out.note(what, &body);
    }
    p.fail(&format!(
        "{what}: Claude did not make the calls asked for, twice"
    ))
}

/// Notes the user record carrying `call`'s result: the launch result's agent id and
/// `backgroundTaskId` live in its `toolUseResult`.
pub fn note_result(p: &mut Probe, heading: &str, call: &Value) {
    let records = p.records();
    let id = call["id"].as_str().unwrap_or_default();
    let body = match tool_result(&records, id) {
        Some(record) => format!("call:\n{call}\n\nresult record:\n{record}"),
        None => format!("call:\n{call}\n\n(no tool_result record for {id})"),
    };
    p.out.note(heading, &body);
}

/// Waits up to `limit` seconds for a task-notification line after the first `from` lines;
/// returns every such line.
pub fn await_notification(p: &Probe, from: usize, limit: u64) -> Vec<String> {
    let found = || {
        let lines = p.lines();
        let new = lines.get(from..).unwrap_or_default();
        containing(new, NOTIFICATION)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    };
    let arrived = || {
        p.allow_pending();
        !found().is_empty()
    };
    p.within(secs(limit), arrived);
    found()
}

/// Waits up to `limit` seconds for a turn after the first `starts`; when one starts, for it
/// to end. Returns whether one started.
pub fn await_opened_turn(p: &Probe, starts: usize, limit: u64, what: &str) -> bool {
    let opened = p.within(secs(limit), || turn_starts(p.r(), &p.id) > starts);
    if opened {
        p.await_turns(starts, what, secs(90));
    }
    opened
}

/// Notes each subagent's meta file and last record, and every SubagentStart and SubagentStop
/// the spool holds.
pub fn note_agents(p: &mut Probe) {
    for (agent, path) in subagent_files(&p.transcript()) {
        let lines = lines_of(&path);
        let records = parsed(&lines);
        let meta = fs::read_to_string(meta_of(&path));
        let meta = meta.unwrap_or_else(|error| format!("(no meta file: {error})"));
        let body = format!(
            "meta file:\n{meta}\n\nrecords: {}\nlast assistant stop_reason: {}\n\nlast record:\n{}",
            lines.len(),
            last_stop_reason(&records),
            lines.last().map(String::as_str).unwrap_or("(none)")
        );
        p.out.note(&format!("Subagent {agent}"), &body);
    }
    let spool = p.spool();
    for event in ["SubagentStart", "SubagentStop"] {
        let found = spooled(&spool, event);
        let typed = found.iter().map(|e| {
            let has = e["payload"].get("agent_type").is_some();
            format!("payload has agent_type: {has}\n{e}")
        });
        let heading = format!("Spool {event} entries: {}", found.len());
        p.out.note(&heading, &joined(typed));
    }
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_background_agent_and_idle_notification() {
    let mut p = Probe::open("p1-background-agent", |_, _| {});
    let ask = format!(
        "Use the Agent tool with run_in_background set to true, subagent_type \
         \"general-purpose\", description \"probe background agent\" and this prompt: \
         \"{SLEEPER}\" Do not wait for it or check on it. Reply with the single word LAUNCHED."
    );
    let wanted: [Wanted; 1] = [("Agent", backgrounded)];
    let calls = prompt_for(&mut p, &ask, "the background Agent launch", &wanted);
    let (lines, starts) = (p.lines().len(), turn_starts(p.r(), &p.id));
    note_result(&mut p, "Background Agent launch result", &calls[0]);
    let found = await_notification(&p, lines, 120);
    if found.is_empty() {
        p.miss("no task notification for the background agent within 120 s");
    }
    let opened = await_opened_turn(&p, starts, 15, "the notification's turn");
    let body = format!("opened a turn: {opened}\n\n{}", joined(&found));
    p.out.note("Task notification arriving while idle", &body);
    p.within(secs(5), || !spooled(&p.spool(), "SubagentStop").is_empty());
    note_agents(&mut p);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_background_bash_and_busy_notification() {
    let mut p = Probe::open("p1-background-bash", |_, _| {});
    let ask = format!(
        "Do these two things in this order in one reply. First, start the shell command \
         `sleep 5; echo BASH-DONE` with the Bash tool with run_in_background set to true. \
         Second, without waiting for it, use the Agent tool in the foreground \
         (run_in_background false), subagent_type \"general-purpose\", description \
         \"probe foreground agent\" and this prompt: \"{SLEEPER}\" When the agent has \
         answered, reply with the single word FINISHED."
    );
    let lines = p.lines().len();
    let wanted: [Wanted; 2] = [("Bash", backgrounded), ("Agent", foreground)];
    let calls = prompt_for(
        &mut p,
        &ask,
        "the background Bash and foreground Agent",
        &wanted,
    );
    let during = await_notification(&p, lines, 0);
    let starts = turn_starts(p.r(), &p.id);
    let found = if during.is_empty() {
        await_notification(&p, lines, 60)
    } else {
        during.clone()
    };
    if found.is_empty() {
        p.miss("no task notification for the background Bash within 60 s of the turn's end");
    }
    let opened = await_opened_turn(&p, starts, 10, "the notification's turn");
    let body = format!(
        "present when the working turn ended: {}\nopened a turn of its own: {opened}\n\n{}",
        !during.is_empty(),
        joined(&found)
    );
    p.out.note("Background Bash task notification", &body);
    note_result(&mut p, "Background Bash launch result", &calls[0]);
    note_result(&mut p, "Foreground Agent result", &calls[1]);
    note_agents(&mut p);
    p.finish();
}
