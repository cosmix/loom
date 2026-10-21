#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "usage_accounting_contracts/support.rs"]
mod support;

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use support::*;

#[test]
fn since_accepts_an_rfc3339_instant() {
    let scratch = Scratch::new();
    let (claude, project) = (scratch.path("claude"), scratch.project("p"));
    let dir = claude.join(slug(&project));
    write_lines(
        &dir.join("s1.jsonl"),
        &[
            assistant("s1", "m1", "2026-10-14T06:00:00Z", 5, json!([])),
            assistant("s1", "m2", "2026-10-14T07:00:00Z", 7, json!([])),
        ],
    );
    let report = scratch.usage(&[
        "--project",
        path_str(&project),
        "--claude-root",
        path_str(&claude),
        "--since",
        "2026-10-14T06:30:00Z",
        "--until",
        "2026-10-14T23:00:00Z",
    ]);
    assert_eq!(output_total(provider(&report, "claude")), Some(7));
}

/// The removed-worktree layout: own slug, a stage worktree slug with a
/// subagent, and a sibling project that shares the prefix.
fn removed_worktree_layout(claude: &Path, project: &Path) {
    let slug = slug(project);
    let ts = "2026-10-14T06:00:00Z";
    write_lines(
        &claude.join(&slug).join("s1.jsonl"),
        &[assistant("s1", "m1", ts, 5, json!([]))],
    );
    let stage = claude.join(format!("{slug}--worktrees-stage-a"));
    write_lines(
        &stage.join("s2.jsonl"),
        &[assistant("s2", "m2", ts, 7, json!([]))],
    );
    write_lines(
        &stage.join("s2/subagents/agent-a1.jsonl"),
        &[subagent("s2", "a1", "m3", ts, 11, json!([]))],
    );
    write_lines(
        &claude.join(format!("{slug}-other")).join("s3.jsonl"),
        &[assistant("s3", "m4", ts, 100, json!([]))],
    );
}

fn claude_window<'a>(project: &'a Path, claude: &'a Path) -> Vec<&'a str> {
    vec![
        "--project",
        path_str(project),
        "--claude-root",
        path_str(claude),
        "--since",
        DAY,
        "--until",
        UNTIL,
    ]
}

#[test]
fn removed_worktree_transcripts_are_counted() {
    let scratch = Scratch::new();
    let (claude, project) = (scratch.path("claude"), scratch.project("p"));
    assert!(!project.join(".worktrees").exists());
    removed_worktree_layout(&claude, &project);
    let report = scratch.usage(&claude_window(&project, &claude));
    let entry = provider(&report, "claude");
    assert_eq!(output_total(entry), Some(23), "{entry}");
    assert_eq!(entry["coverage"]["measured_responses"].as_u64(), Some(3));
}

#[test]
fn unread_session_transcript_reads_partial() {
    let scratch = Scratch::new();
    let (claude, project) = (scratch.path("claude"), scratch.project("p"));
    removed_worktree_layout(&claude, &project);
    let stage_a = claude.join(format!("{}--worktrees-stage-a", slug(&project)));
    write_session_record(
        &project,
        "session-a-1",
        "stage-a",
        &stage_a.join("s2.jsonl"),
    );
    let first = scratch.usage(&claude_window(&project, &claude));
    let entry = provider(&first, "claude");
    assert_eq!(state(entry), "measured", "{entry}");

    let stage_b = claude.join(format!("{}--worktrees-stage-b", slug(&project)));
    write_session_record(
        &project,
        "session-b-1",
        "stage-b",
        &stage_b.join("s4.jsonl"),
    );
    let second = scratch.usage(&claude_window(&project, &claude));
    let entry = provider(&second, "claude");
    assert_eq!(state(entry), "partial", "{entry}");
    assert!(has_reason(entry, "missed-session-transcripts"), "{entry}");
}

const FORWARD_COMMAND: &str = "~/.claude/hooks/loom/codex-forward.sh task 'work' --model gpt-5.6-terra --effort xhigh --write --unit-id unit-a --invocation-id inv-0123456789abcdef0123456789abcdef";

fn replay_payload() -> Value {
    let job = "task-replay-1";
    let stdout = format!(
        "LOOM-FORWARD-START {{\"v\":1,\"backend\":\"companion\",\"job_id\":\"{job}\"}}\nLOOM-FORWARD-END {{\"v\":1,\"backend\":\"companion\",\"job_id\":\"{job}\",\"outcome\":\"succeeded\",\"exit_code\":0}}\n--- LOOM-FORWARD-OUTPUT ---\ndone\n"
    );
    json!({
        "session_id": "parent-1", "agent_id": "fwd-1", "tool_use_id": "toolu_replay_1",
        "cwd": "/w", "hook_event_name": "PostToolUse", "tool_name": "Bash",
        "tool_input": {"command": FORWARD_COMMAND},
        "tool_response": {"stdout": stdout, "stderr": "", "interrupted": false, "isImage": false}
    })
}

fn run_forward_hook(home: &Path, work: &Path, payload: &Value) -> std::process::Output {
    let mut command = helpers::loom_cmd();
    helpers::clear_relay_env(&mut command);
    command
        .args(["hook", "forward-receipt"])
        .env("LOOM_WORK_DIR", work)
        .env("LOOM_STAGE_ID", "stage-a")
        .env("LOOM_SESSION_ID", "session-a")
        .env("LOOM_HOOK_CONTEXT", "1")
        .env("HOME", home)
        .env_remove("CLAUDE_PLUGIN_DATA")
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn loom hook forward-receipt");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(payload.to_string().as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().expect("wait for loom hook")
}

#[test]
fn forward_receipt_forms_from_the_posttooluse_payload() {
    let scratch = Scratch::new();
    let (home, work) = (scratch.path("home"), scratch.path("work"));
    let jobs = home.join(".claude/plugins/data/codex-openai-codex/state/ws-replay/jobs");
    fs::create_dir_all(&jobs).unwrap();
    let job = json!({"id":"task-replay-1","status":"completed","phase":"done","threadId":"thread-replay-1"});
    fs::write(jobs.join("task-replay-1.json"), job.to_string()).unwrap();
    let output = run_forward_hook(&home, &work, &replay_payload());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "hook failed: {stderr}");
    let path = work.join("subagents/stage-a/forward-receipts.jsonl");
    let body = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}; stderr={stderr}"));
    let lines: Vec<Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.iter().any(|line| line["state"] == "running"),
        "{body}"
    );
    assert!(
        lines.iter().any(|line| line["state"] == "succeeded"
            && line["codex_thread_id"] == "thread-replay-1"
            && line["tool_use_id"] == "toolu_replay_1"),
        "{body}"
    );
}

fn codex_run(scratch: &Scratch, scope: &[&str], codex: &Path, claude: &Path) -> Value {
    let mut args = scope.to_vec();
    args.extend([
        "--provider",
        "codex",
        "--codex-root",
        path_str(codex),
        "--claude-root",
        path_str(claude),
        "--since",
        DAY,
        "--until",
        UNTIL,
    ]);
    scratch.usage(&args)
}

#[test]
fn codex_totals_hold_only_the_project_threads() {
    let scratch = Scratch::new();
    let (codex, claude) = (scratch.path("codex"), scratch.path("claude-empty"));
    three_rollout_store(&codex);
    let (a, b) = (scratch.project("a"), scratch.project("b"));
    write_receipts(&a, &["thread-a"]);
    write_receipts(&b, &["thread-b"]);
    for (project, expected) in [(&a, 11), (&b, 13)] {
        let report = codex_run(&scratch, &["--project", path_str(project)], &codex, &claude);
        let entry = provider(&report, "codex");
        assert_eq!(output_total(entry), Some(expected), "{entry}");
        assert_eq!(diagnostic(entry, "foreign_codex_rows"), Some(2), "{entry}");
    }
}

#[test]
fn receipt_without_a_rollout_reads_partial() {
    let scratch = Scratch::new();
    let (codex, claude) = (scratch.path("codex"), scratch.path("claude-empty"));
    write_rollout(&codex, "thread-a", "2026-10-14T10:00:00Z", 11);
    let a = scratch.project("a");
    write_receipts(&a, &["thread-a", "thread-missing"]);
    let report = codex_run(&scratch, &["--project", path_str(&a)], &codex, &claude);
    let entry = provider(&report, "codex");
    assert_eq!(output_total(entry), Some(11), "{entry}");
    assert_eq!(state(entry), "partial", "{entry}");
    assert!(has_reason(entry, "forward-thread-missing"), "{entry}");
}

#[test]
fn codex_scope_is_open_under_all_and_unavailable_without_loom_state() {
    let scratch = Scratch::new();
    let (codex, claude) = (scratch.path("codex"), scratch.path("claude-empty"));
    three_rollout_store(&codex);
    let report = codex_run(&scratch, &["--all"], &codex, &claude);
    let entry = provider(&report, "codex");
    assert_eq!(output_total(entry), Some(41), "{entry}");
    assert_eq!(diagnostic(entry, "foreign_codex_rows"), Some(0), "{entry}");

    let bare = scratch.path("n");
    assert!(!bare.join(".loom/work").exists());
    let report = codex_run(&scratch, &["--project", path_str(&bare)], &codex, &claude);
    let entry = provider(&report, "codex");
    assert_eq!(diagnostic(entry, "foreign_codex_rows"), Some(3), "{entry}");
    assert_eq!(
        diagnostic(entry, "codex_scope_unavailable"),
        Some(1),
        "{entry}"
    );
    assert_eq!(state(entry), "partial", "{entry}");
    assert!(has_reason(entry, "codex-scope-unavailable"), "{entry}");
}

fn write_spawn_ledger(project: &Path) {
    let dir = project.join(".loom/work/subagents/stage-a");
    let spawn = json!({"ts":"2026-10-14T06:00:01.000Z","stage_id":"stage-a","session_id":"session-a","agent_type":"loom-software-engineer","model":"sonnet"});
    let start = json!({"agent_id":"a1","agent_type":"loom-software-engineer","parent_session_id":"s1","loom_session_id":"session-a","ts":"2026-10-14T06:00:02.000Z"});
    write_lines(&dir.join("spawns.jsonl"), &[spawn.to_string()]);
    write_lines(&dir.join("starts.jsonl"), &[start.to_string()]);
}

fn write_section_transcripts(claude: &Path, project: &Path) {
    let stage = claude.join(format!("{}--worktrees-stage-a", slug(project)));
    let watch = json!({
        "type": "assistant", "timestamp": "2026-10-14T06:00:04Z", "sessionId": "s1",
        "message": {"id": "m2", "model": "claude-opus-5-5",
            "content": bash_use("tu2", "loom subagents watch --timeout 60"),
            "usage": {"input_tokens": 40, "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0, "output_tokens": 3}}
    });
    let agent = json!([{"type":"tool_use","id":"tu3","name":"Agent","input":{"subagent_type":"loom-software-engineer","prompt":"work"}}]);
    write_lines(
        &stage.join("s1.jsonl"),
        &[
            assistant(
                "s1",
                "m1",
                "2026-10-14T06:00:00Z",
                2,
                bash_use("tu1", "cargo test --lib x"),
            ),
            tool_result("s1", "2026-10-14T06:00:03Z", "tu1", &"x".repeat(100)),
            watch.to_string(),
            tool_result("s1", "2026-10-14T06:00:05Z", "tu2", "done"),
            assistant("s1", "m3", "2026-10-14T06:00:06Z", 4, agent),
        ],
    );
    let edit = json!([{"type":"tool_use","id":"tu4","name":"Edit","input":{"file_path":"/w/a.rs","old_string":"a","new_string":"b"}}]);
    write_lines(
        &stage.join("s1/subagents/agent-a1.jsonl"),
        &[subagent("s1", "a1", "m4", "2026-10-14T06:00:07Z", 6, edit)],
    );
}

#[test]
fn sections_report_bash_output_observation_orientation_and_requested_model() {
    let scratch = Scratch::new();
    let (claude, project) = (scratch.path("claude"), scratch.project("p"));
    write_spawn_ledger(&project);
    write_section_transcripts(&claude, &project);
    let r = scratch.usage(&claude_window(&project, &claude));
    let commands = r["tools"]["bash_output"]["by_command"].as_array().cloned();
    let row = json!({"command": "cargo test", "calls": 1, "output_bytes": 100});
    assert!(
        commands
            .unwrap_or_default()
            .iter()
            .any(|c| contains(c, &row)),
        "{}",
        r["tools"]
    );
    let observation = &r["polling"]["observation"];
    let wait = json!({"kind": "subagents-wait", "requests": 1, "resident_input_tokens": 40});
    let kinds = observation["by_kind"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(kinds.iter().any(|k| contains(k, &wait)), "{observation}");
    assert_eq!(observation["requests"].as_u64(), Some(1), "{observation}");
    assert_eq!(
        observation["output_tokens"].as_u64(),
        Some(3),
        "{observation}"
    );
    let main = &r["lifecycle"]["orientation"]["main"];
    assert_eq!(main["samples"].as_u64(), Some(1), "{}", r["lifecycle"]);
    assert_eq!(
        main["median_requests"].as_f64(),
        Some(2.0),
        "{}",
        r["lifecycle"]
    );
    let agents = r["agents"]["by_agent_model"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let model = json!({"agent_type": "loom-software-engineer", "requested_model": "sonnet"});
    assert!(
        agents.iter().any(|a| contains(a, &model)),
        "{}",
        r["agents"]
    );
}

/// Whether every key of `subset` holds the same value in `value`.
fn contains(value: &Value, subset: &Value) -> bool {
    subset
        .as_object()
        .unwrap()
        .iter()
        .all(|(key, expected)| &value[key] == expected)
}

#[test]
fn damaged_evidence_reads_partial() {
    let scratch = Scratch::new();
    let (claude, codex, project) = (
        scratch.path("claude"),
        scratch.path("codex"),
        scratch.project("p"),
    );
    let main = claude.join(slug(&project)).join("s1.jsonl");
    write_lines(
        &main,
        &[
            assistant("s1", "m1", "2026-10-14T06:00:00Z", 5, json!([])),
            "{not json".to_string(),
        ],
    );
    let receipts = write_receipts(&project, &["thread-a"]);
    let mut body = fs::read_to_string(&receipts).unwrap();
    body.push_str("{not json\n");
    fs::write(&receipts, body).unwrap();
    write_rollout(&codex, "thread-a", "2026-10-14T10:00:00Z", 11);
    let mut args = claude_window(&project, &claude);
    args.extend(["--codex-root", path_str(&codex), "--provider", "all"]);
    let report = scratch.usage(&args);
    let claude_entry = provider(&report, "claude");
    assert_eq!(output_total(claude_entry), Some(5), "{claude_entry}");
    assert_eq!(state(claude_entry), "partial", "{claude_entry}");
    assert!(has_reason(claude_entry, "malformed-rows"), "{claude_entry}");
    assert!(!has_reason(claude_entry, "malformed-forward-receipts"));
    let codex_entry = provider(&report, "codex");
    assert_eq!(output_total(codex_entry), Some(11), "{codex_entry}");
    assert_eq!(state(codex_entry), "partial", "{codex_entry}");
    assert!(
        has_reason(codex_entry, "malformed-forward-receipts"),
        "{codex_entry}"
    );
}
