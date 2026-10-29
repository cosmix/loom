//! The queue rules of the scheduler table: batches, the limit, questions, buttons, turns.

use std::sync::Mutex;
use std::time::Instant;

use super::*;

/// Records starts and stops; sessions are `s1`, `s2`, ... in start order. While `fail` holds a
/// reason every start fails with it.
#[derive(Default)]
pub(super) struct Stub {
    starts: Mutex<Vec<Vec<String>>>,
    stops: Mutex<Vec<(String, bool)>>,
    pub(super) fail: Mutex<Option<String>>,
}

impl SessionSpawner for Stub {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        if let Some(why) = self.fail.lock().unwrap().clone() {
            anyhow::bail!(why);
        }
        let mut starts = self.starts.lock().unwrap();
        starts.push(ids.to_vec());
        Ok(format!("s{}", starts.len()))
    }

    fn stop(&self, session: &str, release_claims: bool) {
        let stop = (session.to_owned(), release_claims);
        self.stops.lock().unwrap().push(stop);
    }
}

impl Stub {
    pub(super) fn starts(&self) -> Vec<Vec<String>> {
        self.starts.lock().unwrap().clone()
    }

    pub(super) fn stops(&self) -> Vec<(String, bool)> {
        self.stops.lock().unwrap().clone()
    }
}

pub(super) fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| format!("plans/{name}")).collect()
}

/// The server's scheduler, a stub spawner and a clock origin.
pub(super) fn setup() -> (Scheduler, Stub, Instant) {
    let scheduler = Scheduler::new(SchedulerConfig::standard(None));
    (scheduler, Stub::default(), Instant::now())
}

pub(super) fn question_of(events: &[SchedulerEvent]) -> Option<QuestionView> {
    events.iter().rev().find_map(|event| match event {
        SchedulerEvent::Asked(question) => Some(question.clone()),
        _ => None,
    })
}

fn names(count: usize) -> Vec<String> {
    let names: Vec<String> = (1..=count).map(|i| format!("p{i}")).collect();
    ids(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn a_batch_queues_only_fresh_ids() {
    let (mut s, stub, now) = setup();
    s.on_update(&ids(&["a", "b"]), &ids(&["a", "b"]), now, &stub);
    s.seal_seen("plans/a", now);
    s.on_update(&ids(&["d"]), &ids(&["a", "b", "d"]), now, &stub);
    let token = s.suspend(SuspendReason::PressureRun);
    let all = ids(&["a", "b", "c", "d"]);
    s.on_update(&ids(&["a", "b", "c", "d", "c"]), &all, now, &stub);
    let view = s.panel_state();
    assert_eq!(
        view.queue,
        ids(&["d", "a", "c"]),
        "b is still the session's"
    );
    s.resume(token, &all, now, &stub);
    assert_eq!(stub.starts().len(), 1, "one session at a time");
}

#[test]
fn nothing_starts_while_a_question_is_open() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    let mut seven = six.clone();
    seven.push("plans/x".into());
    s.on_update(&ids(&["x"]), &seven, now, &stub);
    assert!(stub.starts().is_empty());
    assert_eq!(s.panel_state().queue, seven, "x waits behind the question");
    assert_eq!(s.panel_state().question.expect("still open").id, asked.id);
}

#[test]
fn the_limit_counts_the_whole_queue() {
    let (mut s, stub, now) = setup();
    let token = s.suspend(SuspendReason::PressureRun);
    let six = names(6);
    s.on_update(&six[..3], &six, now, &stub);
    s.on_update(&six[3..], &six, now, &stub);
    s.resume(token, &six, now, &stub);
    assert!(stub.starts().is_empty(), "three and three are six");
    let question = s.panel_state().question.expect("asks");
    assert_eq!(question.ids, six);
}

#[test]
fn disabled_auto_asks_before_a_single_summary() {
    let (mut s, stub, now) = setup();
    let one = ids(&["a"]);
    s.set_auto(false);

    let asked = question_of(&s.on_update(&one, &one, now, &stub)).expect("asks");

    assert_eq!(asked.ids, one);
    assert!(stub.starts().is_empty());
}

#[test]
fn disabling_auto_holds_an_already_queued_small_batch() {
    let (mut s, stub, now) = setup();
    let one = ids(&["a"]);
    let token = s.suspend(SuspendReason::PressureRun);
    s.on_update(&one, &one, now, &stub);

    s.set_auto(false);
    s.resume(token, &one, now, &stub);

    assert_eq!(s.panel_state().question.expect("asks").ids, one);
    assert!(stub.starts().is_empty());
}

#[test]
fn reenabled_auto_starts_the_next_batch_of_five() {
    let (mut s, stub, now) = setup();
    let one = ids(&["a"]);
    s.set_auto(false);
    let asked = question_of(&s.on_update(&one, &one, now, &stub)).expect("asks");
    s.answer(&asked.id, false, AnswerRight::Holder, now, &stub)
        .expect("declines");
    let five = names(5);

    s.set_auto(true);
    s.on_update(&five, &five, now, &stub);

    assert_eq!(stub.starts(), vec![five]);
    assert!(s.panel_state().question.is_none());
}

#[test]
fn yes_starts_the_asked_ids_and_the_rest_waits() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    let mut seven = six.clone();
    seven.push("plans/x".into());
    s.on_update(&ids(&["x"]), &seven, now, &stub);
    s.answer(&asked.id, true, AnswerRight::Holder, now, &stub)
        .expect("the holder answers");
    assert_eq!(
        stub.starts(),
        vec![six.clone()],
        "only its ids, no second check"
    );
    assert_eq!(s.panel_state().queue, ids(&["x"]));
    six.iter().for_each(|id| s.seal_seen(id, now));
    s.session_ended("s1", now, &stub);
    assert_eq!(stub.starts()[1], ids(&["x"]), "x is a batch of one");
}

#[test]
fn no_declines_the_asked_ids_and_starts_the_rest() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    let mut seven = six.clone();
    seven.push("plans/x".into());
    s.on_update(&ids(&["x"]), &seven, now, &stub);
    s.answer(&asked.id, false, AnswerRight::FreeAuthoring, now, &stub)
        .expect("anyone answers while authoring is free");
    let events = s.take_events();
    assert!(events.contains(&SchedulerEvent::QuestionClosed { id: asked.id }));
    assert_eq!(s.panel_state().declined, six);
    assert_eq!(stub.starts(), vec![ids(&["x"])]);
}

#[test]
fn refused_answers_keep_the_question() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    let wrong = s.answer("q99", true, AnswerRight::Holder, now, &stub);
    assert_eq!(wrong, Err(AnswerRefused::NoSuchQuestion));
    let refused = s.answer(&asked.id, false, AnswerRight::NotAllowed, now, &stub);
    assert_eq!(refused, Err(AnswerRefused::NotAllowed));
    assert_eq!(s.panel_state().question, Some(asked));
    assert!(s.panel_state().declined.is_empty());
}

#[test]
fn buttons_queue_first_in_press_order() {
    let (mut s, stub, now) = setup();
    let token = s.suspend(SuspendReason::PressureRun);
    let all = ids(&["a", "b", "c", "d"]);
    s.on_update(&ids(&["a", "b"]), &all, now, &stub);
    s.press(&ids(&["c"]), now, &stub);
    s.press(&ids(&["d", "a"]), now, &stub);
    assert_eq!(s.panel_state().queue, ids(&["c", "d", "a", "b"]));
    s.resume(token, &all, now, &stub);
    assert_eq!(stub.starts(), vec![ids(&["c", "d", "a", "b"])]);
}

#[test]
fn buttons_skip_the_limit_and_the_batch_waits() {
    let (mut s, stub, now) = setup();
    let token = s.suspend(SuspendReason::PressureRun);
    let nine = names(9);
    let mut all = nine.clone();
    all.extend(ids(&["x", "y"]));
    s.on_update(&ids(&["x", "y"]), &all, now, &stub);
    s.press(&nine, now, &stub);
    s.resume(token, &all, now, &stub);
    assert_eq!(
        stub.starts(),
        vec![nine.clone()],
        "the pressed nine, no question"
    );
    assert!(s.panel_state().question.is_none());
    nine.iter().for_each(|id| s.seal_seen(id, now));
    s.session_ended("s1", now, &stub);
    assert_eq!(stub.starts()[1], ids(&["x", "y"]));
}

#[test]
fn a_button_undeclines() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    s.answer(&asked.id, false, AnswerRight::Holder, now, &stub)
        .expect("declined");
    s.press(&six[..1], now, &stub);
    assert_eq!(stub.starts(), vec![six[..1].to_vec()]);
    assert_eq!(s.panel_state().declined, six[1..].to_vec());
}

#[test]
fn current_summaries_leave_declined_queue_failed_and_question() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    let events = s.on_update(&[], &six[..4], now, &stub);
    let shrunk = question_of(&events).expect("the question names four");
    assert_eq!(
        (shrunk.id.as_str(), shrunk.ids.len()),
        (asked.id.as_str(), 4)
    );
    s.answer(&asked.id, false, AnswerRight::Holder, now, &stub)
        .expect("declined");
    s.press(&six[..1], now, &stub);
    s.session_failed("s1", "the session died", now, &stub);
    assert_eq!(
        s.reason(&six[0]),
        Some(RebuildReason::Failed("the session died".into()))
    );
    s.on_update(&[], &six[1..2], now, &stub);
    let view = s.panel_state();
    assert!(view.failed.is_empty(), "{view:?}");
    assert_eq!(view.declined, six[1..2].to_vec());
    s.on_update(&[], &[], now, &stub);
    assert!(s.panel_state().declined.is_empty());
}

#[test]
fn the_question_closes_when_nothing_is_left_to_ask() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    assert_eq!(s.reason(&six[0]), Some(RebuildReason::AwaitingApproval));
    let events = s.on_update(&[], &[], now, &stub);
    assert!(events.contains(&SchedulerEvent::QuestionClosed { id: asked.id }));
    assert!(s.panel_state().question.is_none());
    assert!(stub.starts().is_empty());
}

#[test]
fn a_turn_end_queues_what_is_still_out_of_date_minus_the_declined() {
    let (mut s, stub, now) = setup();
    let six = names(6);
    let asked = question_of(&s.on_update(&six, &six, now, &stub)).expect("asks");
    s.answer(&asked.id, false, AnswerRight::Holder, now, &stub)
        .expect("declined");
    s.turn_started();
    let mut all = six.clone();
    all.extend(ids(&["x", "y"]));
    s.turn_ended(&all, now, &stub);
    assert_eq!(
        stub.starts(),
        vec![ids(&["x", "y"])],
        "the declined six stay out"
    );
}

#[test]
fn a_turn_end_during_a_pressure_run_only_queues() {
    let (mut s, stub, now) = setup();
    let token = s.suspend(SuspendReason::PressureRun);
    s.turn_started();
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, now, &stub);
    s.turn_ended(&two, now, &stub);
    assert!(stub.starts().is_empty(), "the pressure run still holds");
    assert_eq!(s.panel_state().queue, two);
    s.resume(token, &two, now, &stub);
    assert_eq!(stub.starts(), vec![two]);
}

#[test]
fn idle_ends_an_open_turn_and_a_late_turn_end_is_ignored() {
    let (mut s, stub, now) = setup();
    s.turn_started();
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, now, &stub);
    s.authoring_state_idle("a1", &two, now, &stub);
    assert_eq!(stub.starts(), vec![two.clone()]);
    let mut three = two.clone();
    three.push("plans/c".into());
    s.on_update(&ids(&["c"]), &three, now, &stub);
    s.turn_ended(&[], now, &stub);
    assert_eq!(
        s.panel_state().queue,
        ids(&["c"]),
        "the late end prunes nothing"
    );
}

#[test]
fn idle_without_an_open_turn_changes_nothing() {
    let (mut s, stub, now) = setup();
    let token = s.suspend(SuspendReason::PressureRun);
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, now, &stub);
    s.authoring_state_idle("a1", &[], now, &stub);
    assert!(stub.starts().is_empty(), "the pressure suspension holds");
    assert_eq!(s.panel_state().queue, two, "nothing pruned");
    s.resume(token, &two, now, &stub);
    s.resume(token, &[], now, &stub);
    assert_eq!(
        stub.starts(),
        vec![two],
        "a second resume of a token is ignored"
    );
}
