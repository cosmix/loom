//! Contracts: Codex-bound spawns get no worker brief, a slow brief still
//! reaches the spawn, and compaction resets only the session's prompt record.

#[path = "integration/helpers.rs"]
// only loom_bin_path() and clear_relay_env() are used
#[allow(dead_code)]
mod helpers;

use chrono::Utc;
use loom::context::delivery::{
    hook_recipient_id, load_deliveries, record_delivery, DeliveredNode, DeliveryRecord,
};
use loom::context::local_overlay::local_overlay_key;
use loom::fs::permissions::constants::{
    HOOK_BOUNDED, HOOK_COMMON, HOOK_KNOWLEDGE_ORIENT, HOOK_READ_DISCIPLINE, HOOK_READ_LEDGER,
    HOOK_SPAWN_GUARD, HOOK_SUBAGENT_PREAMBLE,
};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

const NONCE: &str = "0123456789abcdef0123456789abcdef";
const MARKER: &str = "<!-- loom-worker-brief nonce=0123456789abcdef0123456789abcdef -->";
const BRIEF: &str = "## Scoped worker brief\n\n<!-- loom-worker-brief nonce=0123456789abcdef0123456789abcdef -->\n\nScoped fact only.";

const STUB: &str = r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' "$*" >>"$LOOM_CALLS"
[[ "$1" == "hook" && "$2" == "worker-brief" ]] || exit 0
[[ -z "${WORKER_BRIEF_SLEEP:-}" ]] || sleep "$WORKER_BRIEF_SLEEP"
printf '%s\n' "$WORKER_BRIEF_RESPONSE"
"#;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn install(dir: &Path, files: &[(&str, &str)]) {
    for (name, content) in files {
        let path = dir.join(name);
        fs::write(&path, content).unwrap_or_else(|e| panic!("write {name}: {e}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

fn install_spawn_guard() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("create hook dir");
    fs::create_dir_all(temp.path().join("bin")).expect("create stub bin");
    install(
        temp.path(),
        &[
            ("_bounded.sh", HOOK_BOUNDED),
            ("_common.sh", HOOK_COMMON),
            ("_read_discipline.sh", HOOK_READ_DISCIPLINE),
            ("_read_ledger.sh", HOOK_READ_LEDGER),
            ("_subagent-preamble.txt", HOOK_SUBAGENT_PREAMBLE),
            ("spawn-guard.sh", HOOK_SPAWN_GUARD),
            ("bin/loom", STUB),
        ],
    );
    let hook = temp.path().join("spawn-guard.sh");
    (temp, hook)
}

fn run_bash(mut cmd: Command, hook: &Path, payload: &str) -> Out {
    cmd.arg(hook)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn hook");
    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(payload.as_bytes())
        .expect("write payload");
    let output = child.wait_with_output().expect("wait for hook");
    Out {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// One Task spawn through spawn-guard.sh with the PATH stub; returns the
/// hook output and the stub's recorded argv lines.
fn spawn(hook: &Path, tool_input: Value, sleep: Option<&str>) -> (Out, String) {
    let scratch = TempDir::new().expect("create scratch dir");
    let stub_bin = hook.parent().expect("hook dir").join("bin");
    let calls = scratch.path().join("calls");
    let mut cmd = Command::new("bash");
    helpers::clear_relay_env(&mut cmd)
        .current_dir(scratch.path())
        .env_remove("LOOM_HOOK_DEBUG")
        .env("HOME", scratch.path())
        .env("TMPDIR", scratch.path())
        .env("PATH", format!("{}:/usr/bin:/bin", stub_bin.display()))
        .env("LOOM_STAGE_ID", "test-stage")
        .env("LOOM_MAIN_AGENT_PID", std::process::id().to_string())
        .env("LOOM_WORK_DIR", scratch.path())
        .env("LOOM_SESSION_ID", "test-session")
        .env("LOOM_CALLS", &calls)
        .env(
            "WORKER_BRIEF_RESPONSE",
            json!({"nonce": NONCE, "brief": BRIEF}).to_string(),
        );
    if let Some(seconds) = sleep {
        cmd.env("WORKER_BRIEF_SLEEP", seconds);
    }
    let payload = json!({"tool_name": "Task", "tool_input": tool_input}).to_string();
    let out = run_bash(cmd, hook, &payload);
    let recorded = fs::read_to_string(&calls).unwrap_or_default();
    (out, recorded)
}

fn updated_prompt(stdout: &str) -> String {
    let value: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("hook stdout is not one JSON object ({e}): {stdout}"));
    value["hookSpecificOutput"]["updatedInput"]["prompt"]
        .as_str()
        .unwrap_or_else(|| panic!("no updatedInput.prompt: {stdout}"))
        .to_string()
}

#[test]
fn codex_bound_spawns_receive_no_worker_brief() {
    let (_hooks, hook) = install_spawn_guard();
    let codex_bound = [
        json!({"subagent_type": "loom-codex-forwarder", "model": "sonnet",
               "prompt": "LOOM-CODEX-FORWARD-ONLY forward this unit"}),
        json!({"subagent_type": "codex:codex-rescue", "model": "sonnet",
               "prompt": "rescue this unit"}),
    ];
    for input in codex_bound {
        let label = input["subagent_type"].clone();
        let (out, calls) = spawn(&hook, input, None);
        assert!(
            !calls.contains("hook worker-brief"),
            "{label}: spawn guard asked for a worker brief; calls={calls} stderr={}",
            out.stderr
        );
        assert!(
            !out.stdout.contains("loom-worker-brief"),
            "{label}: worker brief marker reached the spawn; stdout={}",
            out.stdout
        );
    }
    let engineer = json!({"subagent_type": "loom-software-engineer", "model": "sonnet",
                          "prompt": "engineer task"});
    let (out, calls) = spawn(&hook, engineer, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        updated_prompt(&out.stdout).contains(MARKER),
        "engineer spawn lost its worker brief; calls={calls} stdout={}",
        out.stdout
    );
}

#[test]
fn slow_worker_brief_still_reaches_the_spawn() {
    let (_hooks, hook) = install_spawn_guard();
    let input = json!({"subagent_type": "loom-software-engineer", "model": "sonnet",
                       "prompt": "slow brief task"});

    let (out, calls) = spawn(&hook, input, Some("3.5"));

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let prompt = updated_prompt(&out.stdout);
    assert!(
        prompt.ends_with(&format!("\n\n{BRIEF}")),
        "a 3.5 s worker brief did not reach the spawn; calls={calls} prompt={prompt}"
    );
}

fn prompt_record(recipient_id: &str) -> DeliveryRecord {
    DeliveryRecord {
        recipient_id: recipient_id.to_string(),
        launch_id: format!("launch-{recipient_id}"),
        context_epoch: "epoch-1".to_string(),
        delivered: vec![DeliveredNode {
            node_id: "node-1".to_string(),
            content_hash: "hash-1".to_string(),
        }],
        written_at: Utc::now(),
    }
}

/// Runs knowledge-orient.sh outside any stage with a compact payload for
/// `session-a` in the checkout at `root`.
fn compact_session_a(root: &Path) -> Out {
    let hooks = TempDir::new().expect("create hook dir");
    install(
        hooks.path(),
        &[
            ("_bounded.sh", HOOK_BOUNDED),
            ("knowledge-orient.sh", HOOK_KNOWLEDGE_ORIENT),
        ],
    );
    let loom_home = TempDir::new().expect("create LOOM_HOME");
    fs::write(
        loom_home.path().join("config.toml"),
        "[update]\ncheck = false\n",
    )
    .expect("write scratch user config");
    let mut cmd = Command::new("bash");
    helpers::clear_relay_env(&mut cmd)
        .current_dir(root)
        .env_remove("LOOM_STAGE_ID")
        .env("LOOM_WORK_DIR", root)
        .env("LOOM_BIN", helpers::loom_bin_path())
        .env("LOOM_HOME", loom_home.path());
    let payload = json!({"source": "compact", "session_id": "session-a", "cwd": root}).to_string();
    run_bash(cmd, &hooks.path().join("knowledge-orient.sh"), &payload)
}

#[test]
fn compacted_ordinary_session_loses_only_its_prompt_record() {
    let root = TempDir::new().expect("create checkout");
    let work = root.path().join(".loom").join("work");
    fs::create_dir_all(&work).expect("create work dir");
    let (plan, stage) = local_overlay_key(root.path());
    let session_a = hook_recipient_id(&stage, Some("session-a"));
    let session_b = hook_recipient_id(&stage, Some("session-b"));
    for recipient in [&session_a, &session_b] {
        record_delivery(&work, &plan, &stage, &prompt_record(recipient)).expect("file record");
    }

    let out = compact_session_a(root.path());

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout={}", out.stdout);
    let left: Vec<String> = load_deliveries(&work, &plan, &stage)
        .expect("load deliveries")
        .into_iter()
        .map(|record| record.recipient_id)
        .collect();
    assert!(
        !left.contains(&session_a),
        "session-a kept its record: {left:?}"
    );
    assert!(
        left.contains(&session_b),
        "session-b lost its record: {left:?}"
    );
}
