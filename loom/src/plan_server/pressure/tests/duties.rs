//! What a run watches and leaves behind: teammate panes and transcripts, the configuration
//! guard, recovery after a dead server, and the shutdown drain.

use std::fs;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use serial_test::serial;

use super::fake::{Call, Script};
use super::{call_on, start_body, wait_for, wait_until, Fixture, PLAN, REPORT};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::pressure::codex::{CONFIG_CHANGED, NOT_EXITED};
use crate::plan_server::pressure::driver::TEAMMATE_PANES;
use crate::plan_server::pressure::install::PressureRuns;
use crate::plan_server::pressure::recover::interrupted_runs;
use crate::plan_server::pressure::teammates::teammate_transcripts;
use crate::plan_server::wire::pressure::{PressureStepKind, RunEndKind};
use crate::plan_session::bridge::{
    EndCause, EndMode, RecoveredSession, RecoveryReport, Transcripts,
};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::TokenCounts;
use crate::plan_session::transcript::Mapper;
use crate::process::is_process_alive;

const TEAMMATE: &str = include_str!("../fixtures/teammate.jsonl");
const TEAMMATE_META: &str = include_str!("../fixtures/teammate.meta.json");

#[test]
#[serial]
fn a_second_tmux_pane_fails_the_pressure_step() {
    let f = Fixture::new();
    fs::write(f.tools.join("panes"), "2").unwrap();
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Failed, "{end:?}");
    assert_eq!(end.step.as_deref(), Some("round 1 /pressure"));
    assert_eq!(end.reason.as_deref(), Some(TEAMMATE_PANES));
    let teardown = Call::End("s-1".into(), EndMode::Teardown, EndCause::Caller);
    assert!(f.fake.calls().contains(&teardown));
}

/// A session transcript directory with one teammate and one subagent.
fn transcripts(f: &Fixture) -> (Transcripts, PathBuf, PathBuf) {
    let dir = f.root.join("transcripts/session/subagents");
    fs::create_dir_all(&dir).unwrap();
    let teammate = dir.join("agent-areviewer-4f2a9c1d7e3b5a60.jsonl");
    fs::write(&teammate, TEAMMATE).unwrap();
    fs::write(
        dir.join("agent-areviewer-4f2a9c1d7e3b5a60.meta.json"),
        TEAMMATE_META,
    )
    .unwrap();
    let subagent = dir.join("agent-a1b2c3.jsonl");
    fs::write(&subagent, "").unwrap();
    fs::write(
        dir.join("agent-a1b2c3.meta.json"),
        r#"{"toolUseId":"toolu_1"}"#,
    )
    .unwrap();
    let found = Transcripts {
        main: Some(f.root.join("transcripts/session.jsonl")),
        subagents: vec![
            ("a1b2c3".into(), subagent.clone()),
            ("areviewer-4f2a9c1d7e3b5a60".into(), teammate.clone()),
        ],
    };
    (found, teammate, subagent)
}

#[test]
#[serial]
fn only_teammate_transcripts_are_registered_for_their_tokens() {
    let f = Fixture::new();
    let (found, teammate, subagent) = transcripts(&f);
    assert_eq!(
        teammate_transcripts(&found),
        std::slice::from_ref(&teammate)
    );
    *f.fake.transcripts.lock().unwrap() = Some(found);
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    let registered = Call::AddTokens("s-1".into(), teammate);
    wait_until("the teammate's registration", || {
        f.fake.calls().contains(&registered)
    });
    let (status, _) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202);
    f.finished();
    let subagent = Call::AddTokens("s-1".into(), subagent);
    assert!(
        !f.fake.calls().contains(&subagent),
        "a subagent was registered as a teammate"
    );
}

#[test]
fn the_teammate_fixture_adds_its_usage_to_the_open_turn_and_no_item() {
    let mut mapper = Mapper::new(PathBuf::from("/repo"));
    mapper.begin_turn(chrono::Utc::now());
    for line in TEAMMATE.lines() {
        mapper.ingest_tokens_only(line);
    }
    let turn = &mapper.turns()[0];
    let expected = TokenCounts {
        input: 2 + 6,
        cache_write: 27026 + 412,
        cache_read: 22262 + 49288,
        output: 180 + 64,
    };
    assert_eq!(turn.tokens, expected);
    assert!(turn.items.is_empty(), "{:?}", turn.items);
}

#[test]
#[serial]
fn a_codex_configuration_edit_blocks_launches_until_confirmed() {
    let f = Fixture::new();
    f.codex("config", "");
    f.start(2);
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Failed, "{end:?}");
    assert_eq!(end.step.as_deref(), Some("round 1 Codex"));
    let reason = format!("{CONFIG_CHANGED}: .claude/settings.local.json");
    assert_eq!(end.reason.as_deref(), Some(reason.as_str()));
    let block = f.ctx.runtime().dir.join("config-block.json");
    assert!(block.is_file(), "no config-block.json");
    let (status, body) = f.call("/api/pressure/start", &f.holder, Some(start_body(PLAN, 1)));
    assert_eq!(
        (status, body["error"].as_str()),
        (409, Some("config_blocked"))
    );
    let restarted = f.restart(RecoveryReport::default());
    let (_, options) = call_on(&restarted, "/api/pressure/options", &f.holder, None);
    assert_eq!(
        options["disabled_reason"].as_str(),
        Some(reason.as_str()),
        "{options}"
    );
    let viewer = BrowserId::mint().unwrap();
    let frames = frames_on(&restarted, &viewer);
    let runs = restarted.extensions().get::<PressureRuns>().unwrap();
    assert!(
        runs.last_view().is_none(),
        "a restarted server has a run view"
    );
    let (status, body) = call_on(&restarted, "/api/pressure/confirm-config", &f.holder, None);
    assert_eq!(status, 200, "{body}");
    assert!(!block.exists(), "the block outlived the confirmation");
    frames_until_cue(&frames);
    let (_, options) = call_on(&restarted, "/api/pressure/options", &viewer, None);
    assert!(options["config_block"].is_null(), "{options}");
}

/// The frames sent to a new tab of `browser` on `ctx`, parsed, in order.
pub(super) fn frames_on(ctx: &Arc<PlanContext>, browser: &BrowserId) -> mpsc::Receiver<Value> {
    let conn = ctx.conn_ctx(browser.clone());
    let incoming = ctx.hub().register(&conn);
    let (sender, frames) = mpsc::channel();
    thread::spawn(move || {
        for frame in incoming {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            if sender.send(frame).is_err() {
                break;
            }
        }
    });
    frames
}

/// Every frame up to and including the next `pressure_options_changed`.
pub(super) fn frames_until_cue(frames: &mpsc::Receiver<Value>) -> Vec<Value> {
    let mut seen = Vec::new();
    loop {
        let frame = frames
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("no pressure_options_changed frame; saw {seen:?}"));
        let cue = frame["type"] == "pressure_options_changed";
        seen.push(frame);
        if cue {
            return seen;
        }
    }
}

#[test]
#[serial]
fn a_run_ending_tells_every_tab_to_fetch_the_options_once_the_slot_is_free() {
    let f = Fixture::new();
    f.codex("report", "");
    let viewer = BrowserId::mint().unwrap();
    let frames = frames_on(&f.ctx, &viewer);
    f.start(1);
    let seen = frames_until_cue(&frames);
    // What a browser does on the cue: fetch the options at once.
    let (_, options) = f.call("/api/pressure/options", &viewer, None);
    assert_eq!(options["running"], false, "{options}");
    let ended = |frame: &Value| frame["type"] == "pressure_run" && !frame["ended"].is_null();
    assert!(
        seen.iter().any(ended),
        "the cue came before the ended view: {seen:?}"
    );
    f.finished();
}

/// SIGKILLs a fake Codex that ignores TERM when dropped, so a failed assertion cannot leak it.
struct KillOnDrop(u32);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let pid = nix::unistd::Pid::from_raw(self.0 as i32);
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    }
}

/// The stopped run left Codex running: the block is persisted and every start refuses on it.
fn assert_not_exited_blocks(f: &Fixture, viewer: &BrowserId, codex_pid: u32) {
    assert!(
        is_process_alive(codex_pid),
        "Codex exited despite ignoring TERM"
    );
    let block = f.ctx.runtime().dir.join("config-block.json");
    assert!(block.is_file(), "no config-block.json");
    let (_, options) = f.call("/api/pressure/options", viewer, None);
    let block = serde_json::json!({ "reason": NOT_EXITED, "changed": [] });
    assert_eq!(options["config_block"], block, "{options}");
    let (status, body) = f.call("/api/pressure/start", &f.holder, Some(start_body(PLAN, 1)));
    assert_eq!(
        (status, body["error"].as_str()),
        (409, Some("config_blocked"))
    );
}

#[test]
#[serial]
fn a_codex_still_running_after_its_grace_blocks_launches_until_confirmed() {
    let f = Fixture::new();
    f.codex("stubborn", "15");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    let codex = wait_for("Codex recorded", || f.fake.children("s-1").first().copied());
    let _kill = KillOnDrop(codex.pid);
    let (status, _) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202);
    assert_eq!(f.ended().kind, RunEndKind::Stopped);
    let viewer = BrowserId::mint().unwrap();
    assert_not_exited_blocks(&f, &viewer, codex.pid);
    let frames = frames_on(&f.ctx, &viewer);
    let (status, body) = f.call("/api/pressure/confirm-config", &f.holder, None);
    assert_eq!(status, 200, "{body}");
    let seen = frames_until_cue(&frames);
    assert!(
        seen.iter().all(|frame| frame["type"] != "pressure_run"),
        "the run view was sent again: {seen:?}"
    );
    let (_, options) = f.call("/api/pressure/options", &viewer, None);
    assert!(options["config_block"].is_null(), "{options}");
}

/// The finish path: the Claude half ends with its marker while Codex, ignoring TERM, outlives
/// the grace after Stop.
#[test]
#[serial]
fn a_codex_outliving_its_grace_after_the_marker_blocks_launches() {
    let f = Fixture::new();
    f.codex("stubborn", "15");
    f.start(1);
    let codex = wait_for("Codex recorded", || f.fake.children("s-1").first().copied());
    let _kill = KillOnDrop(codex.pid);
    let runs = f.runs();
    wait_until("Codex running alone after the marker", || {
        let view = runs.last_view();
        view.is_some_and(|view| view.step == PressureStepKind::Codex)
    });
    let (status, _) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202);
    assert_eq!(f.ended().kind, RunEndKind::Stopped);
    let viewer = BrowserId::mint().unwrap();
    assert_not_exited_blocks(&f, &viewer, codex.pid);
}

fn pressure_session(plan: &str) -> RecoveredSession {
    RecoveredSession {
        id: format!("recovered-{plan}"),
        kind: SessionKind::Pressure { plan: plan.into() },
        children: Vec::new(),
    }
}

#[test]
#[serial]
fn recovery_deletes_an_interrupted_runs_report_and_names_the_run() {
    let f = Fixture::new();
    fs::write(f.root.join(REPORT), "# Review\n").unwrap();
    let outside = f.root.join("../codex-PLAN-x.md");
    fs::write(&outside, "# Elsewhere\n").unwrap();
    let report = RecoveryReport {
        torn_down: vec![pressure_session(PLAN), pressure_session("../PLAN-x.md")],
        closed: vec![pressure_session(PLAN)],
        left_alone: Vec::new(),
    };
    let lines = interrupted_runs(&f.root, &report);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let line = format!("pressure run on {PLAN} was interrupted when the last server stopped");
    assert_eq!(lines[0], line);
    assert!(
        !f.root.join(REPORT).exists(),
        "the report outlived the recovery"
    );
    assert!(outside.exists(), "a planted plan path steered a deletion");
    fs::write(f.root.join(REPORT), "# Review\n").unwrap();
    let _restarted = f.restart(report);
    assert!(
        !f.root.join(REPORT).exists(),
        "install did not hand recovery the report"
    );
}

#[test]
#[serial]
fn the_shutdown_drain_stops_a_running_run_and_returns() {
    let f = Fixture::new();
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
    wait_until("the first step", || {
        !f.fake.started.lock().unwrap().is_empty()
    });
    let drains = f.ctx.shutdown().begin(|_: &str| {});
    drains.join().unwrap();
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Interrupted, "{end:?}");
    assert!(!f.plan_text().contains("## Pressure tests"));
}
