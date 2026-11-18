//! The quiet period: background rebuilds wait for edits to stop; presses and answers do not.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::plan_server::summaries::scheduler::tests::{ids, question_of, setup};
use crate::plan_server::summaries::scheduler::{AnswerRight, QUIET_PERIOD};

const MINUTE: Duration = Duration::from_secs(60);

fn at(t0: Instant, minutes: u32) -> Instant {
    t0 + MINUTE * minutes
}

#[test]
fn a_burst_of_edits_starts_nothing_until_the_quiet_period_passes_after_the_last_edit() {
    let (mut s, stub, t0) = setup();
    let a = ids(&["a"]);
    s.sources_changed(&a, t0);
    s.on_update(&a, &a, t0, &stub);
    for minute in [2, 4] {
        s.sources_changed(&a, at(t0, minute));
        s.on_update(&[], &a, at(t0, minute), &stub);
    }
    assert_eq!(s.waiting(at(t0, 4)), a, "the panel shows it waiting");
    let last = at(t0, 4);
    s.tick(last + QUIET_PERIOD - Duration::from_secs(1), &stub);
    assert!(stub.starts().is_empty(), "every edit restarts the period");
    s.tick(last + QUIET_PERIOD, &stub);
    assert_eq!(
        stub.starts(),
        vec![a.clone()],
        "one rebuild after the burst"
    );
    assert!(s.waiting(last + QUIET_PERIOD).is_empty());
}

#[test]
fn a_press_during_the_quiet_period_starts_at_once() {
    let (mut s, stub, t0) = setup();
    let a = ids(&["a"]);
    s.sources_changed(&a, t0);
    s.on_update(&a, &a, t0, &stub);
    assert!(stub.starts().is_empty());
    s.press(&a, at(t0, 1), &stub);
    assert_eq!(stub.starts(), vec![a]);
}

#[test]
fn a_yes_during_the_quiet_period_starts_the_asked_ids_at_once() {
    let (mut s, stub, t0) = setup();
    let six = ids(&["p1", "p2", "p3", "p4", "p5", "p6"]);
    let asked = question_of(&s.on_update(&six, &six, t0, &stub)).expect("six ask");
    s.sources_changed(&six, at(t0, 1));
    s.on_update(&[], &six, at(t0, 1), &stub);
    s.answer(&asked.id, true, AnswerRight::Holder, at(t0, 1), &stub)
        .expect("the holder answers");
    assert_eq!(stub.starts(), vec![six]);
}

#[test]
fn an_edit_inside_an_authoring_turn_waits_a_full_period_after_the_turn_ends() {
    let (mut s, stub, t0) = setup();
    let a = ids(&["a"]);
    s.turn_started();
    s.sources_changed(&a, t0);
    s.on_update(&a, &a, t0, &stub);
    let end = at(t0, 10);
    s.tick(end, &stub);
    assert_eq!(s.waiting(end), a, "the turn is still open");
    s.turn_ended(&a, end, &stub);
    assert!(stub.starts().is_empty(), "the edit is ten minutes old");
    s.tick(end + QUIET_PERIOD - Duration::from_secs(1), &stub);
    assert!(stub.starts().is_empty(), "the reader may take another turn");
    s.tick(end + QUIET_PERIOD, &stub);
    assert_eq!(stub.starts(), vec![a]);
}

#[test]
fn waiting_ids_neither_start_with_the_ready_ones_nor_count_toward_the_limit() {
    let (mut s, stub, t0) = setup();
    let fresh = ids(&["p1", "p2", "p3"]);
    let old = ids(&["q1", "q2", "q3"]);
    let mut all = fresh.clone();
    all.extend(old.clone());
    s.sources_changed(&fresh, t0);
    let events = s.on_update(&all, &all, t0, &stub);
    assert!(
        question_of(&events).is_none(),
        "three ready ids are within the limit"
    );
    assert_eq!(stub.starts(), vec![old], "no change on record: no wait");
    assert_eq!(s.waiting(t0), fresh);
}

#[test]
fn a_removed_summary_leaves_the_quiet_period() {
    let (mut s, stub, t0) = setup();
    let a = ids(&["a"]);
    s.set_known(a.iter().cloned().collect());
    s.sources_changed(&a, t0);
    s.on_update(&a, &a, t0, &stub);
    s.set_known(BTreeSet::new());
    s.on_update(&[], &[], at(t0, 1), &stub);
    assert!(s.panel_state().queue.is_empty());
    s.set_known(a.iter().cloned().collect());
    s.on_update(&a, &a, at(t0, 2), &stub);
    assert_eq!(
        stub.starts(),
        vec![a],
        "the removed summary left no wait behind"
    );
}

#[test]
fn an_edit_while_the_session_holds_the_summary_waits_after_an_unsealed_end() {
    let (mut s, stub, t0) = setup();
    let a = ids(&["a"]);
    s.set_known(a.iter().cloned().collect());
    s.on_update(&a, &a, t0, &stub);
    assert_eq!(
        stub.starts(),
        vec![a.clone()],
        "no change on record: no wait"
    );
    s.claims_changed("s1", &a, t0, &stub);
    let edit = at(t0, 1);
    s.sources_changed(&a, edit);
    s.on_update(&[], &[], edit, &stub);
    s.session_ended("s1", at(t0, 2), &stub);
    assert_eq!(stub.starts().len(), 1, "the reader is still editing");
    assert_eq!(s.waiting(at(t0, 2)), a, "the panel shows it waiting");
    s.tick(edit + QUIET_PERIOD - Duration::from_secs(1), &stub);
    assert_eq!(stub.starts().len(), 1);
    s.tick(edit + QUIET_PERIOD, &stub);
    assert_eq!(stub.starts(), vec![a.clone(), a]);
}
