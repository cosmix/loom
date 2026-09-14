#[path = "subagent_owned_wait/scenarios.rs"]
mod scenarios;
#[path = "subagent_owned_wait/support.rs"]
mod support;
#[path = "subagent_owned_wait/support_more.rs"]
mod support_more;

use serde_json::Value;
use support::{assert_exit, Fixture, AGENT_ID, LOOM_SESSION, PARENT_UUID, UNIT_ID};

#[test]
fn claude_worker_with_correlated_stop_succeeds() {
    let fixture = Fixture::new("claude-succeeded");
    fixture.write_claude_stop();

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 30);

    assert_exit(&output, 0);
    let events = fixture.events(&output);
    assert_eq!(events.len(), 2, "expected waiting and terminal records");
    assert_eq!(events[0]["outcome"], "waiting");
    assert_eq!(events[1]["outcome"], "succeeded");
    assert_eq!(events[0]["wait_id"], events[1]["wait_id"]);
    assert_eq!(events[0]["parent_session_id"], PARENT_UUID);
    assert_eq!(events[0]["loom_session_id"], LOOM_SESSION);
    assert_ne!(PARENT_UUID, LOOM_SESSION);
    assert_eq!(
        events[0]["workers"],
        serde_json::json!([
            {"kind": "claude", "id": AGENT_ID}
        ])
    );
}

#[test]
fn codex_companion_failed_job_exits_three() {
    let fixture = Fixture::new("codex-failed");
    fixture.write_codex_job("failed");

    let output = fixture.watch(&[&format!("codex:{UNIT_ID}")], 30);

    assert_exit(&output, 3);
    assert_terminal(&fixture.events(&output), "failed");
}

#[test]
fn codex_companion_cancelled_job_exits_three() {
    let fixture = Fixture::new("codex-cancelled");
    fixture.write_codex_job("cancelled");

    let output = fixture.watch(&[&format!("codex:{UNIT_ID}")], 30);

    assert_exit(&output, 3);
    assert_terminal(&fixture.events(&output), "cancelled");
}

#[test]
fn legacy_watch_forms_fail_with_migration_guidance() {
    let fixture = Fixture::new("migration");

    let missing_worker = fixture.run(&["subagents", "watch", "--json"]);
    let legacy_dir = fixture.run(&[
        "subagents",
        "watch",
        "--dir",
        fixture.transcript_dir_str(),
        "--json",
    ]);

    assert_migration_failure(&missing_worker);
    assert_migration_failure(&legacy_dir);
}

#[test]
fn running_worker_timeout_is_not_proof_of_death() {
    let fixture = Fixture::new("timeout");
    fixture.write_codex_job("running");

    let output = fixture.watch(&[&format!("codex:{UNIT_ID}")], 1);

    assert_exit(&output, 2);
    let events = fixture.events(&output);
    assert_terminal(&events, "timed_out");
    let detail = events[1]["detail"].as_str().expect("terminal detail");
    assert!(
        detail.contains("deadline expiry is not proof that a worker died"),
        "{detail}"
    );
}

#[test]
fn unknown_worker_id_exits_five() {
    let fixture = Fixture::new("unknown-worker");

    let output = fixture.watch(&["claude:missing-worker"], 30);

    assert_exit(&output, 5);
    let events = fixture.events(&output);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["outcome"], "unknown");
}

#[test]
fn same_worker_set_with_live_lease_is_already_waiting() {
    let fixture = Fixture::new("already-waiting");
    fixture.write_claude_stop();
    let selector = format!("claude:{AGENT_ID}");
    let first = fixture.watch(&[&selector], 300);
    assert_exit(&first, 0);
    let lease_path = fixture.seed_live_lease(&fixture.events(&first));
    let before = std::fs::read(&lease_path).expect("read seeded lease");

    let output = fixture.watch(&[&selector], 300);

    assert_exit(&output, 4);
    let events = fixture.events(&output);
    assert_eq!(events.len(), 1, "existing wait must not start a monitor");
    assert_eq!(events[0]["outcome"], "already_waiting");
    assert_eq!(std::fs::read(lease_path).expect("reread lease"), before);
}

fn assert_terminal(events: &[Value], expected: &str) {
    assert_eq!(events.len(), 2, "expected waiting and terminal records");
    assert_eq!(events[0]["outcome"], "waiting");
    assert_eq!(events[1]["outcome"], expected);
    assert_eq!(events[0]["wait_id"], events[1]["wait_id"]);
}

fn assert_migration_failure(output: &std::process::Output) {
    assert!(
        !output.status.success(),
        "legacy watch unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("watch no longer polls a transcript directory"),
        "{stderr}"
    );
    assert!(stderr.contains("--worker claude:<agent-id>"), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("succeeded"));
}
