//! Whole runs: the record, Stop, a Codex abort, a failed session, the fallback and the release.

use std::time::Duration;

use serial_test::serial;

use super::fake::{Call, Script};
use super::{call_on, wait_for, wait_until, Fixture, PLAN, REPORT};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::pressure::{
    CodexState, PressureRunView, PressureStepKind, RunEndKind, StepState,
};
use crate::plan_session::bridge::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::process::is_process_alive;

#[test]
#[serial]
fn a_full_two_round_run_records_one_line() {
    let f = Fixture::new();
    f.codex("report", "");
    f.start(2);
    let view = f.finished();
    assert_recorded_once(&f, &view);
    assert_openings(&f);
    assert_steps(&view);
    assert_eq!(
        f.ctx.authoring().holder(),
        None,
        "authoring outlived the run"
    );
    let late = f.connect(&BrowserId::mint().unwrap());
    let resent = late
        .opened
        .iter()
        .find(|frame| frame["type"] == "pressure_run");
    let resent = resent.expect("a new tab gets no run view");
    assert_eq!(resent["ended"]["kind"], "recorded", "{resent}");
}

/// The record is one line under one `## Pressure tests` heading, and the report is gone.
fn assert_recorded_once(f: &Fixture, view: &PressureRunView) {
    let end = view.ended.clone().unwrap();
    assert_eq!(end.kind, RunEndKind::Recorded, "{end:?}");
    let line = end.line.unwrap();
    let text = f.plan_text();
    assert_eq!(text.matches("## Pressure tests").count(), 1, "{text}");
    assert_eq!(text.matches(&line).count(), 1, "{text}");
    assert!(!f.root.join(REPORT).exists(), "the report outlived the run");
}

/// Each round opens `/pressure` then `/address`, in order.
fn assert_openings(f: &Fixture) {
    let openings: Vec<String> = f
        .fake
        .started
        .lock()
        .unwrap()
        .iter()
        .map(|started| started.guidance.opening_message.clone())
        .collect();
    let (pressure, address) = (
        format!("/loom-plan:pressure {PLAN}"),
        format!("/loom-plan:address {PLAN}"),
    );
    assert_eq!(
        openings,
        [pressure.clone(), address.clone(), pressure, address]
    );
}

/// Two rounds of Claude, Codex, Claude steps, all done, with the tokens and the Codex report.
fn assert_steps(view: &PressureRunView) {
    let kinds: Vec<PressureStepKind> = view.steps.iter().map(|step| step.kind.clone()).collect();
    let round = [
        PressureStepKind::ClaudePressure,
        PressureStepKind::Codex,
        PressureStepKind::ClaudeAddress,
    ];
    assert_eq!(kinds, [round.clone(), round].concat());
    assert!(
        view.steps.iter().all(|step| step.state == StepState::Done),
        "{view:?}"
    );
    assert_eq!(view.tokens_total, 4 * 20);
    let codex = view.steps[1].codex.clone().unwrap();
    assert_eq!(
        (codex.state, codex.report_path.as_deref()),
        (CodexState::ReviewWritten, Some(REPORT))
    );
    assert_eq!(view.steps[0].edits, [PLAN]);
}

#[test]
#[serial]
fn each_step_starts_a_pressure_session_with_its_slots_and_marker() {
    let f = Fixture::new();
    f.start(1);
    assert_eq!(f.ended().kind, RunEndKind::Recorded);
    let started = f.fake.started.lock().unwrap();
    assert_eq!(started.len(), 2);
    let slots = [("opus", "high"), ("sonnet", "medium")];
    for (session, (model, effort)) in started.iter().zip(slots) {
        let plan = SessionKind::Pressure { plan: PLAN.into() };
        assert_eq!(session.kind, plan);
        let teams = (
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_owned(),
            "1".to_owned(),
        );
        assert!(
            session.extra_env.contains(&teams),
            "{:?}",
            session.extra_env
        );
        let guidance = &session.guidance;
        assert!(guidance.opening_on_argv, "the opening is pasted");
        assert_eq!(
            (guidance.model.as_str(), guidance.effort.as_str()),
            (model, effort)
        );
        let marker = session.dir.marker();
        assert!(marker.is_absolute());
        let command = format!(
            "touch -- {}",
            shell_escape::escape(marker.to_string_lossy())
        );
        let brief = &session.brief;
        assert!(brief.contains(&command), "{brief}");
    }
}

#[test]
#[serial]
fn a_stop_in_round_one_writes_no_record_and_deletes_the_report() {
    let f = Fixture::new();
    f.codex("report", "");
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(2);
    let codex = wait_for("Codex recorded", || f.fake.children("s-1").first().copied());
    wait_until("the review", || f.root.join(REPORT).exists());
    let (status, body) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202, "{body}");
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Stopped, "{end:?}");
    assert!(end.by.is_some_and(|by| !by.is_empty()));
    assert!(
        !f.plan_text().contains("## Pressure tests"),
        "a stopped run recorded"
    );
    assert!(
        !f.root.join(REPORT).exists(),
        "the report outlived the stop"
    );
    wait_until("Codex terminated", || !is_process_alive(codex.pid));
    let calls = f.fake.calls();
    let interrupt = calls
        .iter()
        .position(|call| *call == Call::Interrupt("s-1".into()));
    let teardown = Call::End("s-1".into(), EndMode::Teardown, EndCause::Caller);
    let end = calls.iter().position(|call| *call == teardown);
    assert!(interrupt.is_some() && interrupt < end, "{calls:?}");
}

#[test]
#[serial]
fn a_stop_after_the_last_address_marker_writes_no_record() {
    let f = Fixture::new();
    let (ctx, holder) = (std::sync::Arc::downgrade(&f.ctx), f.holder.clone());
    // The last `/address` session (s-2) is released after its marker, before the record.
    *f.fake.on_release.lock().unwrap() = Some(Box::new(move |id: &str| {
        if let Some(ctx) = ctx.upgrade().filter(|_| id == "s-2") {
            call_on(&ctx, "/api/pressure/stop", &holder, None);
        }
    }));
    f.start(1);
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Stopped, "{end:?}");
    assert!(
        !f.plan_text().contains("## Pressure tests"),
        "a stopped run recorded: {}",
        f.plan_text()
    );
}

#[test]
#[serial]
fn a_codex_abort_stops_the_run_and_shows_the_log_tail() {
    let f = Fixture::new();
    f.codex("exit", "2");
    f.start(2);
    let view = f.finished();
    let end = view.ended.clone().unwrap();
    assert_eq!(end.kind, RunEndKind::Failed, "{end:?}");
    assert_eq!(end.step.as_deref(), Some("round 1 Codex"));
    let reason = end.reason.unwrap();
    assert_eq!(reason, "codex exited with code 2 — stopping pressure run.");
    let codex = view
        .steps
        .iter()
        .find(|step| step.kind == PressureStepKind::Codex)
        .unwrap();
    assert_eq!(codex.state, StepState::Failed);
    let status = codex.codex.clone().unwrap();
    assert_eq!(status.state, CodexState::Failed);
    assert!(status
        .log_tail
        .unwrap_or_default()
        .contains("codex fake done"));
    assert_eq!(
        f.fake.started.lock().unwrap().len(),
        1,
        "/address ran after the abort"
    );
    assert!(!f.plan_text().contains("## Pressure tests"));
}

#[test]
#[serial]
fn a_session_failing_before_its_marker_stops_the_run() {
    let f = Fixture::new();
    f.codex("sleep", "30");
    let died = "The session stopped unexpectedly";
    f.fake.script(vec![Script::Fail(died.into())]);
    f.start(1);
    let codex = wait_for("Codex recorded", || f.fake.children("s-1").first().copied());
    let end = f.ended();
    assert_eq!(end.kind, RunEndKind::Failed, "{end:?}");
    assert_eq!(end.step.as_deref(), Some("round 1 /pressure"));
    assert_eq!(end.reason.as_deref(), Some(died));
    wait_until("Codex terminated", || !is_process_alive(codex.pid));
    assert!(!f.plan_text().contains("## Pressure tests"));
}

#[test]
#[serial]
fn codex_is_recorded_again_after_a_fallback_and_shows_the_model_that_ran() {
    let f = Fixture::new();
    f.codex("unavailable-gpt-6.1-sol", "");
    f.codex("sleep", "1");
    f.fake
        .script(vec![Script::Marker(Duration::from_millis(1800))]);
    f.start(1);
    let view = f.finished();
    assert_eq!(
        view.ended.clone().unwrap().kind,
        RunEndKind::Recorded,
        "{view:?}"
    );
    let attempts = f.codex_attempts();
    let models: Vec<&str> = attempts.iter().map(|(_, model)| model.as_str()).collect();
    assert_eq!(models, ["gpt-6.1-sol", "gpt-6-sol"]);
    let recorded: Vec<u32> = f
        .fake
        .children("s-1")
        .iter()
        .map(|child| child.pid)
        .collect();
    let pids: Vec<u32> = attempts.iter().map(|(pid, _)| *pid).collect();
    assert_eq!(recorded, pids);
    let codex = view
        .steps
        .iter()
        .find(|step| step.kind == PressureStepKind::Codex)
        .unwrap();
    assert_eq!(codex.codex.clone().unwrap().model, "gpt-6-sol");
    assert!(f.plan_text().contains("gpt-6-sol"), "{}", f.plan_text());
}

#[test]
#[serial]
fn a_marker_end_keeps_codex_alive_and_releases_once_after_the_join() {
    let f = Fixture::new();
    f.codex("sleep", "2");
    let observed: std::sync::Arc<std::sync::Mutex<Vec<(bool, bool)>>> = Default::default();
    let (seen, dir) = (observed.clone(), f.ctx.runtime().sessions.clone());
    let bin = f.bin.clone();
    *f.fake.on_release.lock().unwrap() = Some(Box::new(move |id: &str| {
        let pids = std::fs::read_to_string(bin.join("pids")).unwrap_or_default();
        let pid: u32 = pids
            .split_whitespace()
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        let log = dir.join(id).join("codex.log").is_file();
        seen.lock().unwrap().push((is_process_alive(pid), log));
    }));
    f.start(1);
    wait_until("the marker end", || {
        f.fake.ended_with("s-1") == Some(EndOutcome::Marker)
    });
    let codex = f.codex_attempts()[0].0;
    assert!(is_process_alive(codex), "the marker end ended Codex");
    let log = f.ctx.runtime().sessions.join("s-1/codex.log");
    assert!(std::fs::read_to_string(log).is_ok(), "codex.log is gone");
    assert!(f
        .fake
        .calls()
        .iter()
        .all(|call| *call != Call::Release("s-1".into())));
    assert_eq!(f.ended().kind, RunEndKind::Recorded);
    let releases = f
        .fake
        .calls()
        .into_iter()
        .filter(|call| *call == Call::Release("s-1".into()));
    assert_eq!(releases.count(), 1);
    assert_eq!(
        observed.lock().unwrap()[0],
        (false, true),
        "released before the join"
    );
}
