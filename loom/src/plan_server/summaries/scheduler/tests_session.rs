//! The running session's rules: claims, seals, stalls, ends, failed starts, server stop.

use std::time::Duration;

use super::tests::{ids, question_of, setup};
use super::*;

fn minutes(at: Instant, minutes: u64) -> Instant {
    at + Duration::from_secs(minutes * 60)
}

#[test]
fn a_gap_between_claim_waves_does_not_stop_the_session() {
    let (mut s, stub, now) = setup();
    let mut batch = ids(&["d1", "d2"]);
    batch.push("engineering".into());
    s.on_update(&batch, &batch, now, &stub);
    s.claims_changed("s1", &batch[..2], now, &stub);
    s.claims_changed("s1", &[], now, &stub);
    assert!(stub.stops().is_empty(), "the overview is not claimed yet");
    s.claims_changed("s1", &batch[2..], now, &stub);
    s.claims_changed("s1", &[], now, &stub);
    s.claims_changed("other", &[], now, &stub);
    assert_eq!(stub.stops(), vec![("s1".to_owned(), false)]);
    let stopped = SchedulerEvent::Stopped {
        session: "s1".into(),
        reason: StopReason::ClaimsDone,
    };
    assert!(s.take_events().contains(&stopped));
    let late = s.tick(minutes(now, 30), &stub);
    assert!(late.is_empty(), "a stopping session does not stall");
}

#[test]
fn a_stall_reports_the_last_seal_error() {
    let (mut s, stub, t0) = setup();
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, t0, &stub);
    let error = "drafts/plans/b.md:3: unknown citation".to_owned();
    s.note_seal_error("s1", Some(error.clone()));
    assert_eq!(s.stall_due(minutes(t0, 9)), None);
    assert_eq!(s.stall_due(minutes(t0, 10)), Some("s1"));
    let events = s.tick(minutes(t0, 10), &stub);
    let failed = SchedulerEvent::Failed {
        ids: two.clone(),
        error: error.clone(),
    };
    assert!(events.contains(&failed), "{events:?}");
    assert_eq!(stub.stops(), vec![("s1".to_owned(), true)]);
    s.session_ended("s1", minutes(t0, 11), &stub);
    assert!(
        s.panel_state().queue.is_empty(),
        "failed ids are not retried"
    );
    assert_eq!(
        s.panel_state().failed,
        vec![(two[0].clone(), error.clone()), (two[1].clone(), error)]
    );
}

#[test]
fn an_ended_session_requeues_its_unsealed_ids_first() {
    let (mut s, stub, now) = setup();
    let ab = ids(&["a", "b"]);
    s.on_update(&ab, &ab, now, &stub);
    let abc = ids(&["a", "b", "c"]);
    s.on_update(&ids(&["c"]), &abc, now, &stub);
    s.seal_seen("plans/a", now);
    assert_eq!(s.progress(), Some((1, 2)));
    s.session_ended("s1", now, &stub);
    assert_eq!(stub.starts()[1], ids(&["b", "c"]), "b first, then c");
    let events = s.take_events();
    let ended = SchedulerEvent::Stopped {
        session: "s1".into(),
        reason: StopReason::Ended,
    };
    assert!(events.contains(&ended), "{events:?}");
}

#[test]
fn a_failed_session_fails_its_unsealed_ids() {
    let (mut s, stub, now) = setup();
    let ab = ids(&["a", "b"]);
    s.on_update(&ab, &ab, now, &stub);
    s.seal_seen("plans/a", now);
    s.session_failed("s1", "The session stopped unexpectedly", now, &stub);
    let events = s.take_events();
    let failed = SchedulerEvent::Failed {
        ids: ids(&["b"]),
        error: "The session stopped unexpectedly".into(),
    };
    assert!(events.contains(&failed), "{events:?}");
    assert!(s.panel_state().running.is_none());
    assert!(s.panel_state().queue.is_empty());
    assert_eq!(stub.starts().len(), 1, "failed ids are not retried");
}

#[test]
fn a_failed_start_keeps_the_ids_queued_and_retries_on_the_next_batch() {
    let (mut s, stub, now) = setup();
    *stub.fail.lock().unwrap() = Some("start blocked: a pressure run is starting".into());
    let two = ids(&["a", "b"]);
    let events = s.on_update(&two, &two, now, &stub);
    assert!(events.is_empty(), "{events:?}");
    let view = s.panel_state();
    assert_eq!(view.queue, two);
    assert_eq!(
        view.start_error.as_deref(),
        Some("start blocked: a pressure run is starting")
    );
    *stub.fail.lock().unwrap() = None;
    s.on_update(&[], &two, now, &stub);
    assert_eq!(stub.starts(), vec![two]);
    assert_eq!(s.panel_state().start_error, None, "cleared by the start");
}

#[test]
fn a_pending_start_resolves_to_the_real_session_or_back_to_the_queue() {
    let (mut s, stub, now) = setup();
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, now, &stub);
    assert_eq!(s.running_session(), Some("s1"));
    assert_eq!(
        s.resolve_start("other", Ok("x".into())),
        None,
        "not the pending one"
    );
    assert_eq!(s.resolve_start("s1", Ok("s9".into())), Some("s9".into()));
    assert_eq!(s.running_session(), Some("s9"));

    let (mut s, stub, now) = setup();
    s.on_update(&two, &two, now, &stub);
    s.press(&ids(&["c"]), now, &stub);
    let refused = Err(anyhow::anyhow!("a summary session is running"));
    assert_eq!(s.resolve_start("s1", refused), None);
    let view = s.panel_state();
    assert_eq!(view.running, None);
    assert_eq!(
        view.queue,
        ids(&["c", "a", "b"]),
        "the button keeps its place"
    );
    assert_eq!(
        view.start_error.as_deref(),
        Some("a summary session is running")
    );
}

#[test]
fn the_server_stopping_drops_the_queue_and_starts_nothing() {
    let (mut s, stub, now) = setup();
    let two = ids(&["a", "b"]);
    s.on_update(&two, &two, now, &stub);
    let mut nine: Vec<String> = (1..=9).map(|i| format!("product/section_0{i}")).collect();
    let asked = question_of(&s.on_update(&nine, &nine, now, &stub));
    assert!(asked.is_none(), "s1 runs: the nine wait");
    s.server_stopping();
    let events = s.take_events();
    let stopping = SchedulerEvent::Stopped {
        session: "s1".into(),
        reason: StopReason::ServerStopping,
    };
    assert_eq!(events, vec![stopping]);
    assert!(stub.stops().is_empty(), "the bridge's drain ends s1");
    s.session_ended("s1", now, &stub);
    nine.extend(two);
    s.on_update(&nine, &nine, now, &stub);
    s.press(&nine, now, &stub);
    assert_eq!(stub.starts().len(), 1, "nothing starts after the stop");
    assert!(s.panel_state().queue.is_empty());
    assert!(s.panel_state().question.is_none());
}

#[test]
fn the_server_stopping_closes_an_open_question() {
    let (mut s, stub, now) = setup();
    let nine: Vec<String> = (1..=9).map(|i| format!("product/section_0{i}")).collect();
    let asked = question_of(&s.on_update(&nine, &nine, now, &stub)).expect("asks");
    s.server_stopping();
    let closed = SchedulerEvent::QuestionClosed { id: asked.id };
    assert_eq!(s.take_events(), vec![closed]);
}
