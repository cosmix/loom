use super::support::{assert_exit, Fixture, AGENT_ID, LOOM_SESSION, PARENT_UUID};
use super::support_more::{PARENT_B, WORKER_B};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::symlink;

#[test]
fn two_claude_parents_require_session_and_never_cross_bind_evidence() {
    let fixture = Fixture::new("two-claude-parents");
    fixture.add_claude_worker(PARENT_B, AGENT_ID);
    fixture.write_stop_for(PARENT_B, AGENT_ID);
    let selector = format!("claude:{AGENT_ID}");

    let ambiguous = fixture.watch(&[&selector], 1);
    let parent_a = fixture.watch_session(&[&selector], 1, PARENT_UUID);
    let parent_b = fixture.watch_session(&[&selector], 1, PARENT_B);

    assert_exit(&ambiguous, 5);
    assert_eq!(only_outcome(&fixture.events(&ambiguous)), "unknown");
    assert_exit(&parent_a, 5);
    assert_ne!(last_outcome(&fixture.events(&parent_a)), "succeeded");
    assert!(fixture
        .events(&parent_a)
        .iter()
        .all(|event| event["parent_session_id"] == PARENT_UUID));
    assert_exit(&parent_b, 0);
    assert_eq!(last_outcome(&fixture.events(&parent_b)), "succeeded");
}

#[test]
fn newer_unrelated_transcript_does_not_change_bound_worker_or_outcome() {
    let fixture = Fixture::new("newer-unrelated-transcript");
    fixture.write_claude_stop();
    fixture.add_newer_unrelated_transcript();

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 30);

    assert_exit(&output, 0);
    let events = fixture.events(&output);
    assert_eq!(last_outcome(&events), "succeeded");
    assert!(events
        .iter()
        .all(|event| event["parent_session_id"] == PARENT_UUID));
}

#[test]
fn loom_session_id_is_rejected_as_claude_parent_session() {
    let fixture = Fixture::new("loom-session-rejected");

    let output = fixture.watch_session(&[&format!("claude:{AGENT_ID}")], 30, LOOM_SESSION);

    assert_exit(&output, 5);
    assert_eq!(only_outcome(&fixture.events(&output)), "unknown");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("succeeded"));
}

#[test]
fn mixed_claude_and_codex_both_succeeded_exits_zero() {
    let fixture = Fixture::new("mixed-succeeded");
    fixture.write_claude_stop();
    fixture.write_codex_job("completed");

    let output = fixture.watch(&mixed_selectors(), 30);

    assert_exit(&output, 0);
    assert_eq!(last_outcome(&fixture.events(&output)), "succeeded");
}

#[test]
fn mixed_claude_and_codex_with_one_running_times_out() {
    let fixture = Fixture::new("mixed-running");
    fixture.write_claude_stop();
    fixture.write_codex_job("running");

    let output = fixture.watch(&mixed_selectors(), 1);

    assert_exit(&output, 2);
    assert_eq!(last_outcome(&fixture.events(&output)), "timed_out");
}

#[test]
fn teammate_idle_evidence_is_nonterminal_and_times_out() {
    let fixture = Fixture::new("teammate-idle");
    fixture.write_idle_for(PARENT_UUID, AGENT_ID);

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 1);

    assert_exit(&output, 2);
    let events = fixture.events(&output);
    assert_eq!(last_outcome(&events), "timed_out");
    assert!(events.iter().all(|event| event["outcome"] != "succeeded"));
}

#[test]
fn assistant_turn_after_correlated_stop_invalidates_old_success() {
    let fixture = Fixture::new("turn-after-stop");
    fixture.write_claude_stop();
    fixture.append_worker_turn(PARENT_UUID, AGENT_ID);

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 1);

    assert_ne!(output.status.code(), Some(0));
    assert!(fixture
        .events(&output)
        .iter()
        .all(|event| event["outcome"] != "succeeded"));
}

#[test]
fn duplicate_restart_replay_is_idempotent_and_wrong_session_is_unknown() {
    let replayed = Fixture::new("duplicate-restart-replay");
    replayed.write_claude_stop();
    replayed.replay_lifecycle_journal();

    let duplicate = replayed.watch(&[&format!("claude:{AGENT_ID}")], 30);

    assert_exit(&duplicate, 0);
    assert_eq!(last_outcome(&replayed.events(&duplicate)), "succeeded");

    let wrong = Fixture::new("wrong-session-replay");
    wrong.write_wrong_loom_session_stop(PARENT_UUID, AGENT_ID);

    let output = wrong.watch(&[&format!("claude:{AGENT_ID}")], 1);

    assert_exit(&output, 5);
    assert_eq!(last_outcome(&wrong.events(&output)), "unknown");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("succeeded"));
}

#[test]
fn different_worker_set_with_live_lease_is_busy_with_existing_wait_id() {
    let fixture = Fixture::new("different-set-busy");
    fixture.write_claude_stop();
    fixture.add_claude_worker(PARENT_UUID, WORKER_B);
    fixture.write_stop_for(PARENT_UUID, WORKER_B);
    let first_selector = format!("claude:{AGENT_ID}");
    let first = fixture.watch_session(&[&first_selector], 30, PARENT_UUID);
    assert_exit(&first, 0);
    fixture.seed_live_lease(&fixture.events(&first));

    let output = fixture.watch_session(&[&format!("claude:{WORKER_B}")], 30, PARENT_UUID);

    assert_exit(&output, 4);
    let events = fixture.events(&output);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["outcome"], "busy");
    assert_eq!(events[0]["wait_id"], "live-owned-wait");
}

#[test]
fn malformed_lease_is_nonzero_and_left_unchanged() {
    let fixture = Fixture::new("malformed-lease");
    let malformed = br#"{"schema_version":1,"wait_id":"unterminated""#;
    let path = fixture.prepare_malformed_lease(malformed);
    let before = fs::read(&path).expect("read malformed lease");

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 30);

    assert_ne!(output.status.code(), Some(0));
    assert_eq!(fs::read(path).expect("reread malformed lease"), before);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("succeeded"));
}

#[test]
fn symlinked_scratch_lease_directory_component_is_refused() {
    let fixture = Fixture::new("symlinked-scratch-component");
    let target = fixture.tmp.join("wait-root-target");
    let lease_path = fixture.lease_dir().join("lease.json");
    fs::create_dir(&target).expect("create symlink target");
    symlink(&target, fixture.scratch_wait_root()).expect("symlink scratch component");

    let output = fixture.watch(&[&format!("claude:{AGENT_ID}")], 30);

    assert_ne!(output.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing symlink directory component")
    );
    assert!(!lease_path.exists());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("succeeded"));
}

fn mixed_selectors() -> [&'static str; 2] {
    // The constants are fixed fixture identifiers, so these selectors can be static.
    ["claude:worker-a", "codex:unit-a"]
}

fn only_outcome(events: &[Value]) -> &str {
    assert_eq!(events.len(), 1);
    events[0]["outcome"].as_str().expect("event outcome")
}

fn last_outcome(events: &[Value]) -> &str {
    events
        .last()
        .and_then(|event| event["outcome"].as_str())
        .expect("terminal event outcome")
}
