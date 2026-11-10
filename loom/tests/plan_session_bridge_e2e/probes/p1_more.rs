//! P1 (plan step A1), continued: an interrupt while a foreground Agent, a background Agent and
//! a background Bash run; a finished agent resumed through `SendMessage`; a compact while a
//! task runs.

use std::thread;

use serde_json::Value;
use serial_test::serial;

use super::evidence::{backgrounded, containing, joined, spooled, subagent_files, tool_result};
use super::p1::{
    await_notification, await_opened_turn, calls_since, foreground, note_agents, note_result,
    prompt_for, Wanted,
};
use super::probe::Probe;
use super::procs::{cmdline, sleeper, tree};
use super::secs;
use crate::diagnose::{turn_ends, turn_starts};

const LONG_SLEEPER: &str =
    "Run the shell command `sleep 45` with the Bash tool, then reply with the single word DONE.";

/// The three calls the interrupt scenario asks for, in their order. Claude Code 2.1.321
/// launches every Agent call async (a "foreground" request included), so the call that keeps
/// the turn running at the interrupt is a foreground Bash. Its Bash tool refuses a foreground
/// standalone `sleep`, so that call waits with `timeout … tail -f /dev/null`.
const THREE: [Wanted; 3] = [
    ("Bash", backgrounded),
    ("Agent", backgrounded),
    ("Bash", foreground),
];

/// The command lines (program and arguments, never the environment) of the session's live
/// `sleep` and `tail -f` processes; a wrapper that merely names them in an argument is skipped.
fn sleepers(p: &Probe) -> Vec<String> {
    let lines = tree(p.pid).into_iter().map(|proc| cmdline(proc.pid));
    lines.filter(|line| sleeper(line)).collect()
}

/// The three calls, once all were made and the foreground Agent's result is still missing.
fn launched(p: &Probe, before: usize) -> Option<Vec<Value>> {
    let records = p.records();
    let new = records.get(before..).unwrap_or_default();
    let calls: Option<Vec<Value>> = THREE
        .iter()
        .map(|(tool, keep)| {
            super::evidence::tool_uses(new, tool, keep)
                .into_iter()
                .next()
        })
        .collect();
    let calls = calls?;
    let running = tool_result(&records, calls[2]["id"].as_str()?).is_none();
    running.then_some(calls)
}

/// Sends the three-call prompt and returns the calls while the foreground Agent runs; a turn
/// that ends first is retried once.
fn launch_three(p: &mut Probe, ask: &str) -> Vec<Value> {
    for attempt in 1..=2 {
        let (before, ends) = (p.records().len(), turn_ends(p.r(), &p.id));
        p.send(ask);
        let mut calls = None;
        let ready = || {
            p.allow_pending();
            calls = launched(p, before);
            calls.is_some() || turn_ends(p.r(), &p.id) > ends
        };
        p.until("the three launches or the turn's end", secs(90), ready);
        if let Some(calls) = calls {
            return calls;
        }
        p.settle("the turn that launched too little", secs(60));
        p.out.note(
            "Interrupt setup",
            &format!("attempt {attempt}: not all three ran"),
        );
    }
    p.fail("the three launches: Claude did not make them as asked, twice")
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_interrupt_with_running_tasks() {
    let mut p = Probe::open("p1-interrupt", |_, _| {});
    let ask = format!(
        "Make these three tool calls in one reply, in this order, without waiting between \
         them: 1. the Bash tool with run_in_background set to true and the command \
         `sleep 45; echo BASH-DONE`; 2. the Agent tool with run_in_background set to true, \
         subagent_type \"general-purpose\", description \"probe background agent\", prompt \
         \"{LONG_SLEEPER}\"; 3. the Bash tool in the foreground (no run_in_background) with \
         the command `timeout 45 tail -f /dev/null; echo FG-DONE`. Then reply with the single \
         word FINISHED."
    );
    let calls = launch_three(&mut p, &ask);
    p.linger(secs(4));
    let (spool, lines, before) = (p.spool().len(), p.lines().len(), sleepers(&p));
    let stopped = p.r().bridge.interrupt(&p.id);
    p.settle("the interrupted turn", secs(60));
    let sizes = || {
        subagent_files(&p.transcript())
            .iter()
            .map(|(_, f)| size(f))
            .sum::<u64>()
    };
    let grown_from = sizes();
    p.linger(secs(15));
    let records = p.records();
    let fg = calls[2]["id"].as_str().unwrap_or_default();
    let fg_result = tool_result(&records, fg).map_or("(none)".to_owned(), Value::to_string);
    let body = format!(
        "interrupt returned: {stopped:?}\n\nforeground Bash tool_result:\n{fg_result}\n\n\
         sleep and tail -f processes before the interrupt:\n{}\n\n15 s after it:\n{}\n\n\
         subagent transcripts grew after the interrupt: {} -> {} bytes\n\n\
         spool after the interrupt:\n{}\n\ntranscript records after it:\n{}",
        joined(&before),
        joined(sleepers(&p)),
        grown_from,
        sizes(),
        joined(p.spool_after(spool)),
        joined(p.lines().iter().skip(lines))
    );
    p.out.note("Interrupt with three tasks running", &body);
    note_result(&mut p, "Background Bash launch result", &calls[0]);
    note_result(&mut p, "Background Agent launch result", &calls[1]);
    note_agents(&mut p);
    p.finish();
}

fn size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map_or(0, |meta| meta.len())
}

/// The agent id of `call`'s result, else of the last SubagentStart spooled.
fn agent_of(p: &Probe, call: &Value) -> Option<String> {
    let records = p.records();
    let result = tool_result(&records, call["id"].as_str()?);
    let from_result = result.and_then(|r| r.pointer("/toolUseResult/agentId")?.as_str());
    let spool = p.spool();
    let started = spooled(&spool, "SubagentStart");
    let from_spool = started
        .last()
        .and_then(|e| e["payload"]["agent_id"].as_str());
    from_result.or(from_spool).map(str::to_owned)
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_send_message_resumes_a_finished_agent() {
    let mut p = Probe::open("p1-resume", |_, _| {});
    let ask = "Use the Agent tool in the foreground, subagent_type \"general-purpose\", \
               description \"probe resumable agent\" and the prompt \"Reply with the single \
               word ONE.\" Then tell me the agent's answer.";
    let wanted: [Wanted; 1] = [("Agent", foreground)];
    let first = prompt_for(&mut p, ask, "the first Agent run", &wanted);
    note_result(&mut p, "First Agent run result", &first[0]);
    let agent = agent_of(&p, &first[0]);
    let agent = agent.unwrap_or_else(|| p.fail("the first Agent run named no agent id"));
    let starts = spooled(&p.spool(), "SubagentStart").len();
    let ask = format!(
        "Use the SendMessage tool to send the message \"Reply with the single word TWO.\" \
         to the agent you just ran, whose agent id is {agent}. Then tell me its answer."
    );
    let any: fn(&Value) -> bool = |_| true;
    // A plan session may have no SendMessage tool at all; that is an answer, not a failure.
    let before = p.records().len();
    p.run_turn(&ask, "the SendMessage resume");
    let Some(resume) = calls_since(&p.records(), before, &[("SendMessage", any)]) else {
        let reply = joined(p.lines().iter().skip(before));
        let body = format!("no SendMessage call was made; records after the ask:\n{reply}");
        p.out.note("SendMessage unavailable", &body);
        note_agents(&mut p);
        return p.finish();
    };
    note_result(&mut p, "SendMessage call and result", &resume[0]);
    let again = || spooled(&p.spool(), "SubagentStart").len() > starts;
    if !p.within(secs(10), again) {
        p.miss("no second SubagentStart after SendMessage");
    }
    note_agents(&mut p);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p1_compact_while_a_task_runs() {
    let mut p = Probe::open("p1-compact", |_, _| {});
    let ask = "Start the shell command `sleep 30; echo BASH-DONE` with the Bash tool with \
               run_in_background set to true, then, without waiting for it, reply with the \
               single word STARTED.";
    let wanted: [Wanted; 1] = [("Bash", backgrounded)];
    let calls = prompt_for(&mut p, ask, "the background Bash launch", &wanted);
    let (spool, lines, starts) = (p.spool().len(), p.lines().len(), turn_starts(p.r(), &p.id));
    p.send("/compact");
    let compacted = || {
        let all = p.lines();
        !containing(all.get(lines..).unwrap_or_default(), "compact_boundary").is_empty()
    };
    p.until("the compact boundary", secs(100), compacted);
    await_opened_turn(&p, starts, 5, "a turn the compact started");
    let found = await_notification(&p, lines, 60);
    if found.is_empty() {
        p.miss("no task notification for the background Bash after the compact");
    }
    let starts = turn_starts(p.r(), &p.id);
    let opened = await_opened_turn(&p, starts, 10, "the notification's turn");
    thread::sleep(secs(1));
    let body = format!(
        "notification opened a turn: {opened}\n\nnotification lines:\n{}\n\n\
         spool after /compact:\n{}\n\ntranscript records after /compact:\n{}",
        joined(&found),
        joined(p.spool_after(spool)),
        joined(p.lines().iter().skip(lines))
    );
    p.out.note("Compact while a background Bash runs", &body);
    note_result(&mut p, "Background Bash launch result", &calls[0]);
    p.finish();
}
