//! Operator runs of the planning server's execution handoff against the real tools: a
//! pressure run with a logged-in Claude Code and Codex, a stopped run, and a commit followed by
//! a run start. Ignored by default; each one skips when a tool is missing, and fails instead
//! under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use loom::plan_server::run::changes::split_changes;
use loom::plan_server::run::commit::commit_doc_changes;
use loom::plan_session::bridge::{Bridge, BridgeEvent};
use loom::plan_session::kind::SessionKind;
use loom::process::sandbox_probe::skip_unless;
use serde_json::json;

#[path = "plan_handoff_operator/common.rs"]
mod common;

use common::{
    pressure_panes, put, running, serve, spawn_loom_plan, start_run, tools_ready, Scratch, MISSING,
    PLAN, REPORT,
};

#[test]
#[ignore = "needs a logged-in Claude Code, codex with its pressure skill, tmux and network"]
#[serial_test::serial]
fn operator_pressure_run_records_one_line() {
    if skip_unless(
        tools_ready(),
        "operator_pressure_run_records_one_line",
        MISSING,
    ) {
        return;
    }
    let scratch = Scratch::new();
    let (server, browser) = serve(&scratch);
    let bridge = server.ctx.extensions().get::<Bridge>().unwrap();
    let first = first_session(&bridge);
    start_run(&browser);
    let deadline = Instant::now() + Duration::from_secs(60 * 60);
    let mut counts = Vec::new();
    while running(&browser) {
        assert!(
            Instant::now() < deadline,
            "the run did not end within 60 minutes"
        );
        counts.extend(pressure_panes(&server, &bridge));
        std::thread::sleep(Duration::from_secs(5));
    }
    assert_eq!(scratch.records(), 1, "exactly one new record line");
    assert!(
        counts.iter().all(|count| *count <= 1),
        "a pressure session opened panes: {counts:?}"
    );
    let id = first
        .recv_timeout(Duration::from_secs(1))
        .expect("no pressure session started");
    let transcript = bridge
        .transcripts(&id)
        .and_then(|found| found.main)
        .unwrap();
    let text = std::fs::read_to_string(transcript).unwrap();
    assert!(
        text.contains("<command-name>"),
        "the opening is not the command's expansion"
    );
    assert!(!text.contains(&format!("\"content\":\"/loom-plan:pressure {PLAN}\"")));
    server.stop().unwrap();
}

/// The id of the first pressure session the bridge starts.
fn first_session(bridge: &Bridge) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    bridge.subscribe(Box::new(move |event: &BridgeEvent| {
        if let BridgeEvent::Started {
            id,
            kind: SessionKind::Pressure { .. },
        } = event
        {
            let _ = sender.send(id.clone());
        }
    }));
    receiver
}

#[test]
#[ignore = "needs a logged-in Claude Code, codex with its pressure skill, tmux and network"]
#[serial_test::serial]
fn operator_stopped_run_writes_no_record() {
    if skip_unless(
        tools_ready(),
        "operator_stopped_run_writes_no_record",
        MISSING,
    ) {
        return;
    }
    let scratch = Scratch::new();
    let (server, browser) = serve(&scratch);
    let bridge = server.ctx.extensions().get::<Bridge>().unwrap();
    let (sender, turns) = mpsc::channel();
    bridge.subscribe(Box::new(move |event: &BridgeEvent| {
        if matches!(event, BridgeEvent::TurnStarted { .. }) {
            let _ = sender.send(());
        }
    }));
    start_run(&browser);
    turns
        .recv_timeout(Duration::from_secs(600))
        .expect("the first step never started a turn");
    let (status, body) = browser.post("/api/pressure/stop", json!({}));
    assert_eq!(status, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(120);
    while running(&browser) {
        assert!(Instant::now() < deadline, "the stopped run did not end");
        std::thread::sleep(Duration::from_secs(1));
    }
    assert_eq!(scratch.records(), 0, "a stopped run recorded");
    assert!(
        !scratch.root.join(REPORT).exists(),
        "the Codex report outlived the stop"
    );
    server.stop().unwrap();
}

#[test]
#[ignore = "needs a logged-in Claude Code, codex with its pressure skill, tmux and network"]
#[serial_test::serial]
fn operator_commit_and_start_run() {
    if skip_unless(tools_ready(), "operator_commit_and_start_run", MISSING) {
        return;
    }
    let scratch = Scratch::new();
    put(
        &scratch.root,
        "doc/notes.md",
        "# Notes\n\nOperator notes.\n",
    );
    let (current, _) = split_changes(&scratch.root).unwrap();
    let ticked = vec!["doc/notes.md".to_owned()];
    let code = commit_doc_changes(
        &scratch.root,
        &ticked,
        "Add notes",
        &current,
        &mut |_: &str| {},
    );
    assert_eq!(code.unwrap(), 0);
    let (mut child, browser) = spawn_loom_plan(&scratch);
    let body = json!({ "plan": PLAN, "backend": "tmux", "clean": false });
    let (status, answer) = browser.post("/api/run/start", body);
    assert!(status < 300, "{status}: {answer}");
    let deadline = Instant::now() + Duration::from_secs(300);
    while !scratch.root.join(".loom/work").exists() {
        assert!(
            Instant::now() < deadline,
            "loom init never created .loom/work"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
    child.stop();
    let loom = |args: &[&str]| {
        let mut command = scratch.loom();
        command.args(args).env("LOOM_CLEAN_YES", "1");
        command.status().unwrap()
    };
    let _ = loom(&["stop"]);
    assert!(
        loom(&["clean", "--all"]).success(),
        "loom clean --all failed"
    );
}
