//! Contracts for the timing rules of a standard stage's in-session `loom check`:
//! a check while a worker is out and a second full check once a review round
//! is recorded are refused before any request ticket is written, `--force`
//! overrides both, and a reviewer being out does not refuse.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "host_job_fixture/mod.rs"]
#[allow(dead_code)]
mod host_job_fixture;

use std::fs;
use std::path::Path;
use std::process::Output;

use host_job_fixture::{build_fixture, Fixture, SESSION, STAGE};
use loom::verify::review::store::{write_round, ReviewRound, RECORD_VERSION};
use serial_test::serial;

/// The exit code of a timing refusal.
const REFUSED: i32 = 3;

fn start_worker(work_dir: &Path, agent_type: &str, agent_id: &str) {
    let dir = work_dir.join("subagents").join(STAGE);
    fs::create_dir_all(&dir).unwrap();
    let row = serde_json::json!({
        "stage_id": STAGE,
        "loom_session_id": SESSION,
        "parent_session_id": "parent",
        "agent_id": agent_id,
        "agent_type": agent_type,
    });
    fs::write(dir.join("starts.jsonl"), format!("{row}\n")).unwrap();
}

fn record_round(work_dir: &Path) {
    let round = ReviewRound {
        version: RECORD_VERSION,
        round: 1,
        agent_id: "reviewer-1".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: "sha256:0000".to_string(),
        base: String::new(),
        behaviour: Default::default(),
        files: Default::default(),
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    write_round(work_dir, STAGE, &round).unwrap();
}

fn check(fx: &Fixture, flags: &[&str]) -> Output {
    let mut command = fx.session_loom();
    command.args(["check", STAGE]).args(flags);
    command.output().unwrap()
}

fn tickets(fx: &Fixture) -> usize {
    fs::read_dir(&fx.scratch_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "req"))
        .count()
}

fn assert_requested(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(
        stdout.starts_with("Host job: requested (check, "),
        "{stdout}"
    );
}

#[test]
#[serial]
fn a_worker_out_refuses_before_any_ticket_and_force_relays() {
    let fx = build_fixture(&["true"]);
    start_worker(&fx.work_dir, "loom-software-engineer", "agent-7");

    let refused = check(&fx, &[]);

    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(REFUSED), "{stderr}");
    assert!(
        stderr.contains("loom-software-engineer agent-7"),
        "{stderr}"
    );
    assert!(stderr.contains("--force"), "{stderr}");
    assert_eq!(tickets(&fx), 0, "a refusal writes no ticket");

    assert_requested(&check(&fx, &["--force"]));
    assert_eq!(tickets(&fx), 1);
}

#[test]
#[serial]
fn a_reviewer_out_does_not_refuse() {
    let fx = build_fixture(&["true"]);
    start_worker(&fx.work_dir, "loom-code-reviewer", "reviewer-1");

    assert_requested(&check(&fx, &[]));
}

#[test]
#[serial]
fn a_recorded_round_refuses_a_full_check_but_not_a_criterion() {
    let fx = build_fixture(&["true"]);
    record_round(&fx.work_dir);

    let refused = check(&fx, &[]);

    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(REFUSED), "{stderr}");
    assert!(stderr.contains("loom stage complete"), "{stderr}");
    assert_eq!(tickets(&fx), 0, "a refusal writes no ticket");

    assert_requested(&check(&fx, &["--criterion", "0"]));
}

#[test]
#[serial]
fn force_overrides_a_recorded_round() {
    let fx = build_fixture(&["true"]);
    record_round(&fx.work_dir);

    assert_requested(&check(&fx, &["--force"]));
}
