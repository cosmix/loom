//! Frozen contract: the owning wait reports how long an active native Codex
//! worker has gone without provider progress.
//!
//! Pinned production symbol (defined in, or imported into, the `wait` module
//! so that `super::emit_active_native_progress` resolves, and called from
//! `own_wait`'s continuing-poll path):
//!
//! ```text
//! fn emit_active_native_progress(
//!     evidence: &LifecycleEvidence,          // the same evidence reader own_wait polls
//!     workers: &[BoundWorker],               // the wait's bound workers
//!     now: chrono::DateTime<chrono::Utc>,    // injected clock; never read inside
//!     last_emit: &mut Option<DateTime<Utc>>, // throttle state: None before the first emit
//!     out: &mut dyn std::io::Write,          // human sink; any `&mut impl Write` also passes
//! ) -> std::io::Result<()>
//! ```
//!
//! Behaviour pinned:
//! - Only workers whose native outcome (`native::outcome_for(.., now, stall_budget)`)
//!   is `Active` get a line; one line per worker, naming it `codex:<unit>`, that
//!   ends in `progress <n>s ago` or `no progress yet` (no trailing text).
//! - `<n>` is `now` minus the newest ThreadStarted, TurnStarted or Progress event;
//!   admission, boundary, control (Message, ServerRequestRefused, ...) events
//!   never count. A future provider time clamps to 0.
//! - Lines are written when `last_emit` is `None` or at least 30 s old; a call
//!   that writes sets `last_emit` to `Some(now)`; a call inside the interval
//!   writes nothing and leaves `last_emit` alone.
//! - `WorkerOutcome` is unchanged: no progress field is added to it.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Duration as Span, TimeZone, Utc};

use super::emit_active_native_progress;
use super::evidence::LifecycleEvidence;
use super::model::{BoundWorker, WorkerKind, WorkerSpec};
use super::native::{outcome_for, state_of};
use super::native_tests::{invocation_id, released, seed, terminal};
use crate::native_worker::model::{EventRecord, MessageState, NativeEvent, Outcome};
use crate::subagent_lifecycle::WorkerOutcome;

const BUDGET: Duration = Duration::from_secs(600);

fn fixed_now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_100_000, 0).unwrap()
}

fn worker(unit: &str, serial: u8) -> BoundWorker {
    BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Codex,
            id: unit.into(),
        },
        lifecycle_identity: None,
        native_invocation: Some(format!("stage-a/{}", invocation_id(serial))),
        authority: None,
        evidence: Vec::new(),
    }
}

/// Seed one invocation, then replace its event log with `(offset, event)`
/// records stamped `now + offset` seconds, so every age is deterministic.
fn seed_timed(
    work: &Path,
    unit: &str,
    serial: u8,
    now: DateTime<Utc>,
    timed: &[(i64, NativeEvent)],
) {
    let dir = seed(work, work, unit, serial, &[]);
    let mut text = String::new();
    for (index, (offset, event)) in timed.iter().enumerate() {
        let record = EventRecord {
            seq: index as u64 + 1,
            at: now + Span::seconds(*offset),
            event: event.clone(),
        };
        text.push_str(&serde_json::to_string(&record).unwrap());
        text.push('\n');
    }
    std::fs::write(dir.join("events.jsonl"), text).unwrap();
}

fn queued_message() -> NativeEvent {
    NativeEvent::Message {
        message_id: "msg-1".into(),
        state: MessageState::Queued,
        turn_id: None,
        detail: None,
    }
}

/// unit-a: provider progress 42 s ago, newer infrastructure events after it.
/// unit-b: admission and boundary events only. unit-c: provider progress
/// stamped in the future. unit-d: finished, so not active.
fn seed_workers(work: &Path, now: DateTime<Utc>) -> Vec<BoundWorker> {
    let thread = NativeEvent::ThreadStarted {
        thread_id: "thread-1".into(),
    };
    let turn = NativeEvent::TurnStarted {
        turn_id: "turn-1".into(),
    };
    let refused = NativeEvent::ServerRequestRefused {
        method: "item/tool".into(),
    };
    let a = [
        (-300, NativeEvent::Admitted),
        (-290, NativeEvent::BoundaryCreated),
        (-100, thread.clone()),
        (-60, turn),
        (-42, NativeEvent::Progress),
        (-30, queued_message()),
        (-20, refused),
        (-10, NativeEvent::BoundaryCreated),
    ];
    seed_timed(work, "unit-a", 1, now, &a);
    let b = [
        (-50, NativeEvent::Admitted),
        (-40, NativeEvent::BoundaryCreated),
        (-10, queued_message()),
    ];
    seed_timed(work, "unit-b", 2, now, &b);
    let c = [(-5, NativeEvent::Admitted), (30, thread)];
    seed_timed(work, "unit-c", 3, now, &c);
    let d = [
        (-60, NativeEvent::Admitted),
        (-30, terminal(Outcome::Succeeded)),
        (-20, released()),
    ];
    seed_timed(work, "unit-d", 4, now, &d);
    vec![
        worker("unit-a", 1),
        worker("unit-b", 2),
        worker("unit-c", 3),
        worker("unit-d", 4),
    ]
}

fn emit(
    evidence: &LifecycleEvidence,
    workers: &[BoundWorker],
    now: DateTime<Utc>,
    last_emit: &mut Option<DateTime<Utc>>,
) -> String {
    let mut out: Vec<u8> = Vec::new();
    emit_active_native_progress(evidence, workers, now, last_emit, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn line_for<'a>(text: &'a str, unit: &str) -> Option<&'a str> {
    let name = format!("codex:{unit}");
    text.lines().find(|line| line.contains(&name))
}

fn assert_statuses_stay_active(work: &Path, workers: &[BoundWorker], now: DateTime<Utc>) {
    for worker in &workers[..3] {
        let state = state_of(work, worker).unwrap();
        assert_eq!(
            outcome_for(&state, now, BUDGET),
            WorkerOutcome::Active,
            "{}",
            worker.worker.id
        );
    }
    let done = state_of(work, &workers[3]).unwrap();
    assert_eq!(outcome_for(&done, now, BUDGET), WorkerOutcome::Succeeded);
}

#[test]
fn an_active_codex_worker_reports_its_progress_age() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let now = fixed_now();
    let workers = seed_workers(work, now);
    let evidence = LifecycleEvidence {
        work_dir: work.to_path_buf(),
        stall_budget: BUDGET,
    };
    let mut last_emit = None;

    let first = emit(&evidence, &workers, now, &mut last_emit);
    let a = line_for(&first, "unit-a").expect(&first);
    assert!(
        a.ends_with("progress 42s ago"),
        "infrastructure reset the age: {a}"
    );
    let b = line_for(&first, "unit-b").expect(&first);
    assert!(b.ends_with("no progress yet"), "{b}");
    let c = line_for(&first, "unit-c").expect(&first);
    assert!(
        c.ends_with("progress 0s ago"),
        "future time must clamp: {c}"
    );
    assert!(line_for(&first, "unit-d").is_none(), "not active: {first}");
    assert_eq!(first.lines().count(), 3, "{first}");

    let inside = emit(&evidence, &workers, now + Span::seconds(10), &mut last_emit);
    assert!(inside.is_empty(), "throttled within 30s: {inside}");

    let later = now + Span::seconds(45);
    let again = emit(&evidence, &workers, later, &mut last_emit);
    let a = line_for(&again, "unit-a").expect(&again);
    assert!(a.ends_with("progress 87s ago"), "{a}");

    assert_statuses_stay_active(work, &workers, now);
}
