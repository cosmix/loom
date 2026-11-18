//! A press and a "Rebuild all" answer hand their session to the bridge at once, are costed
//! `button`, and a press made while a session runs joins the next start.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::tests::{owned, update, Row};
use super::tests_alive::{usage_records, wait_until, Live};
use super::tests_bridge::{ended_with, Rig, ID};
use super::*;
use crate::plan_server::summaries::scheduler::{AnswerRight, SchedulerConfig, SuspendReason};
use crate::plan_session::bridge::EndOutcome;
use crate::plan_session::kind::SessionKind;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::evaluate::SummaryState::{Current, Missing, Rebuilding, Stale};

/// More summaries than the limit: a background batch of them opens a question.
const SIX: [&str; 6] = [
    "plans/p1", "plans/p2", "plans/p3", "plans/p4", "plans/p5", "plans/p6",
];
const STALE: &str = "plans/stale";
const CURRENT: &str = "plans/current";

/// The engine reports `SIX` missing and `ID` current; returns the question that opens.
fn ask_about_six(rebuilds: &Rebuilds) -> String {
    let mut rows: Vec<Row<'_>> = SIX.iter().map(|id| (*id, Missing, None, None)).collect();
    rows.push((ID, Current, Some("A"), None));
    rebuilds.on_update(&update(&rows, &SIX));
    rebuilds.panel().question.expect("six ask").id
}

/// `act`, after `setup`, starts `ids` in the facade's spawner, the bridge holds the session when
/// `act` returns, and the bridge's driver spawns it with no further event.
fn reaches_the_spawn(
    case: &str,
    setup: impl FnOnce(&Rebuilds) -> Option<String>,
    act: impl FnOnce(&Rebuilds, Option<String>),
    ids: &[&str],
) -> Live {
    let live = Live::new();
    live.rig.probe.start_in(&live.rig.bridge);
    let question = setup(&live.rig.rebuilds);
    assert!(live.rig.starts().is_empty(), "{case}: nothing ran before");
    let asked = Instant::now();
    act(&live.rig.rebuilds, question);
    assert_eq!(live.rig.starts(), vec![owned(ids)], "{case}: the start");
    let mut sessions = live.rig.bridge.sessions().into_iter();
    let handed = sessions.any(|(_, kind, _)| kind == SessionKind::Summary);
    assert!(
        handed,
        "{case}: the bridge holds the session when the call returns"
    );
    wait_until(case, || live.host.spawns.load(Ordering::SeqCst) == 1);
    let took = asked.elapsed();
    let bound = Duration::from_secs(1);
    assert!(
        took < bound,
        "{case}: spawned {took:?} later; the driver ticks every 20 ms"
    );
    live
}

#[test]
fn a_pressed_rebuild_reaches_the_spawner_within_one_tick() {
    let press = |rebuilds: &Rebuilds, _: Option<String>| {
        rebuilds.enqueue(owned(&[ID]), Trigger::Button);
    };
    reaches_the_spawn("a press", |_| None, press, &[ID]);
    let answer = |rebuilds: &Rebuilds, question: Option<String>| {
        let question = question.expect("a question");
        let answered = rebuilds.answer(&question, true, AnswerRight::Holder);
        answered.expect("the holder answers");
    };
    let ask = |rebuilds: &Rebuilds| Some(ask_about_six(rebuilds));
    reaches_the_spawn("a Rebuild all answer", ask, answer, &SIX);
    let case = "a press while a question asks about a background batch";
    let live = reaches_the_spawn(case, ask, press, &[ID]);
    let panel = live.rig.rebuilds.panel();
    let open = panel.question.expect("the batch is still asked");
    assert_eq!(
        open.ids,
        owned(&SIX),
        "{case}: the question keeps the batch"
    );
}

/// Start one session through `start`, end it through attribution and read the trigger of the
/// one line the ledger then holds.
fn costed(start: impl FnOnce(&Arc<Rebuilds>)) -> Trigger {
    let live = Live::build(SchedulerConfig::standard(None), true);
    let id = live.start(SessionKind::Summary);
    live.give_transcript(&id, &usage_records(40));
    live.rig.probe.answer_next_start(&id);
    start(&live.rig.rebuilds);
    assert_eq!(live.rig.starts().len(), 1, "one session started");
    live.rig
        .event(ended_with(&id, SessionKind::Summary, EndOutcome::Closed));
    let log = cost::read_log(&live.rig.paths).expect("the ledger");
    assert_eq!(log.len(), 1, "the session's own line: {log:?}");
    log[0].trigger
}

#[test]
fn a_pressed_session_is_costed_as_button() {
    let press = costed(|rebuilds| rebuilds.enqueue(owned(&[ID]), Trigger::Button));
    let approved = costed(|rebuilds| {
        let question = ask_about_six(rebuilds);
        let answered = rebuilds.answer(&question, true, AnswerRight::Holder);
        answered.expect("the holder answers");
    });
    let joined = costed(|rebuilds| {
        let held = rebuilds.suspend(SuspendReason::PressureRun);
        let rows = [(ID, Missing, None, None), (STALE, Stale, Some("S"), None)];
        rebuilds.on_update(&update(&rows, &[STALE]));
        rebuilds.enqueue(owned(&[ID]), Trigger::Button);
        drop(held);
        let running = rebuilds.panel().running.expect("one session");
        assert_eq!(running.ids, owned(&[ID, STALE]), "the background id joined");
    });
    let cases = [
        ("a press", press),
        ("Rebuild all", approved),
        ("joined", joined),
    ];
    for (case, trigger) in cases {
        assert_eq!(trigger, Trigger::Button, "{case}");
    }
}

/// `s1` rebuilds `ID`; then `STALE` and `CURRENT` are pressed and wait for it.
fn pressed_while_s1_runs() -> Rig {
    let rig = Rig::new(false);
    let rows: [Row<'_>; 3] = [
        (ID, Missing, None, None),
        (STALE, Stale, Some("S"), None),
        (CURRENT, Current, Some("C"), None),
    ];
    rig.rebuilds.on_update(&update(&rows, &[ID]));
    rig.rebuilds
        .enqueue(owned(&[STALE, CURRENT]), Trigger::Button);
    assert_eq!(
        rig.starts(),
        vec![owned(&[ID])],
        "no second start while s1 runs"
    );
    let panel = rig.rebuilds.panel();
    assert_eq!(
        panel.running.map(|running| running.session).as_deref(),
        Some("s1")
    );
    assert_eq!(
        panel.queue,
        owned(&[STALE, CURRENT]),
        "the panel names the wait"
    );
    rig
}

#[test]
fn a_press_during_a_running_session_joins_the_next_start() {
    // s1 seals `ID`: the engine's batch shows it current, and s1 has nothing left.
    let rig = pressed_while_s1_runs();
    let sealed: [Row<'_>; 3] = [
        (ID, Current, Some("A"), None),
        (STALE, Stale, Some("S"), None),
        (CURRENT, Current, Some("C"), None),
    ];
    rig.rebuilds.on_update(&update(&sealed, &[]));
    assert_eq!(rig.stops(), vec![("s1".to_owned(), false)], "s1 is done");
    assert_eq!(rig.starts().len(), 1, "s1 has not ended yet");
    let queue = rig.rebuilds.panel().queue;
    assert_eq!(queue, owned(&[STALE, CURRENT]), "both presses still wait");
    rig.event(ended_with("s1", SessionKind::Summary, EndOutcome::Closed));
    assert_eq!(rig.starts()[1], owned(&[STALE, CURRENT]), "after a seal");

    // s1 ends on its own, `ID` claimed and unsealed.
    let rig = pressed_while_s1_runs();
    let claimed: [Row<'_>; 3] = [
        (ID, Rebuilding, None, Some("s1")),
        (STALE, Stale, Some("S"), None),
        (CURRENT, Current, Some("C"), None),
    ];
    rig.rebuilds.on_update(&update(&claimed, &[]));
    rig.event(ended_with("s1", SessionKind::Summary, EndOutcome::Closed));
    let next = rig.starts().get(1).cloned().unwrap_or_default();
    assert_eq!(next, owned(&[STALE, CURRENT, ID]), "after an end");
}
