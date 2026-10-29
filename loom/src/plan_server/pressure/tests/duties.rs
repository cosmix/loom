//! What a run watches and leaves behind: teammate panes and transcripts, the configuration
//! guard, recovery after a dead server, and the shutdown drain.

use std::fs;
use std::path::PathBuf;

use serial_test::serial;

use super::fake::{Call, Script};
use super::{call_on, start_body, wait_until, Fixture, PLAN, REPORT};
use crate::plan_server::pressure::codex::CONFIG_CHANGED;
use crate::plan_server::pressure::driver::TEAMMATE_PANES;
use crate::plan_server::pressure::recover::interrupted_runs;
use crate::plan_server::pressure::teammates::teammate_transcripts;
use crate::plan_server::wire::pressure::RunEndKind;
use crate::plan_session::bridge::{
    EndCause, EndMode, RecoveredSession, RecoveryReport, Transcripts,
};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::TokenCounts;
use crate::plan_session::transcript::Mapper;

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
    let (status, body) = call_on(&restarted, "/api/pressure/confirm-config", &f.holder, None);
    assert_eq!(status, 200, "{body}");
    assert!(!block.exists(), "the block outlived the confirmation");
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
