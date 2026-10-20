//! Contracts: an issued worker brief files a record `load_deliveries` returns,
//! and the worker query stays under its byte cap on a character boundary.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used
#[allow(dead_code)]
mod helpers;

use loom::commands::hook::worker_brief::{worker_query_text, WORKER_QUERY_MAX_BYTES};
use loom::context::delivery::{delivery_dir, load_deliveries, plan_key, worker_recipient_id};
use loom::models::stage::Stage;
use loom::verify::transitions::create_stage;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;

fn brief_stage() -> Stage {
    Stage {
        id: "brief-stage".to_string(),
        name: "Brief stage".to_string(),
        plan_id: Some("PLAN-brief".to_string()),
        skills: vec!["loom-rust".to_string()],
        ..Stage::default()
    }
}

/// Runs the real `loom hook worker-brief` for one engineer spawn and parses
/// its stdout as `{nonce, brief}`.
fn run_worker_brief(root: &Path, work: &Path) -> Value {
    let payload = json!({
        "tool_name": "Task",
        "tool_input": {"subagent_type": "loom-software-engineer",
                       "prompt": "Implement a topic absent from all indexes",
                       "description": "brief contract"},
        "session_id": "claude-parent",
        "cwd": root,
        "transcript_path": root.join("t.jsonl"),
    })
    .to_string();
    let mut child = helpers::loom_cmd()
        .args(["hook", "worker-brief"])
        .current_dir(root)
        .env("LOOM_STAGE_ID", "brief-stage")
        .env("LOOM_WORK_DIR", work)
        .env("LOOM_SESSION_ID", "parent-1")
        .env("LOOM_HOOK_CONTEXT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn loom hook worker-brief");
    let mut stdin = child.stdin.take().expect("hook stdin");
    stdin.write_all(payload.as_bytes()).expect("write payload");
    drop(stdin);
    let output = child.wait_with_output().expect("wait for worker-brief");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn pending_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".pending"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn issued_worker_brief_files_a_loadable_record() {
    let root = TempDir::new().expect("create checkout");
    let work = root.path().join(".loom").join("work");
    fs::create_dir_all(&work).expect("create work dir");
    let stage = brief_stage();
    create_stage(&stage, &work).expect("create stage");

    let value = run_worker_brief(root.path(), &work);

    let nonce = value["nonce"].as_str().expect("nonce in stdout");
    assert!(value["brief"].is_string(), "no brief in stdout: {value}");
    let records = load_deliveries(&work, plan_key(&stage), "brief-stage").expect("load records");
    let expected = worker_recipient_id("brief-stage", "parent-1", nonce);
    assert_eq!(records.len(), 1, "records={records:?}");
    assert_eq!(records[0].recipient_id, expected);
    let worker_dir = delivery_dir(&work, plan_key(&stage), "brief-stage").join("worker-brief");
    let pending = pending_files(&worker_dir);
    assert!(pending.is_empty(), "pending worker records: {pending:?}");
}

#[test]
fn worker_query_stays_under_the_cap() {
    let stage = Stage {
        id: "q-stage".to_string(),
        name: "Query stage".to_string(),
        description: Some("stage words ".repeat(4_000)),
        ..Stage::default()
    };

    let query = worker_query_text(&stage, "worker task", &"prompt words ".repeat(400));

    assert_eq!(WORKER_QUERY_MAX_BYTES, 2_048);
    assert!(
        query.len() <= WORKER_QUERY_MAX_BYTES,
        "query is {} bytes",
        query.len()
    );
    assert!(
        query.starts_with("worker task\nprompt words "),
        "query={query}"
    );
}

#[test]
fn worker_query_cuts_on_a_char_boundary() {
    let stage = Stage {
        id: "q-stage".to_string(),
        ..Stage::default()
    };
    let prompt = format!("{}ééé", "a".repeat(2_045));

    let query = worker_query_text(&stage, "d", &prompt);

    assert!(query.len() <= 2_048, "query is {} bytes", query.len());
    assert!(
        format!("d\n{prompt}").starts_with(&query),
        "query is not a prefix of the joined text"
    );
}
