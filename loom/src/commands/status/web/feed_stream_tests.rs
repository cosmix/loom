//! Whole-stream checks of the feed: a scripted sequence and a subscriber that
//! joins while snapshots are being published.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::super::broadcast::Broadcaster;
use super::super::model::WebSnapshot;
use super::tests::{fixture, kinds, reconstruct, stage_mut, Harness};

/// A deterministic, varied sequence touching every kind of change.
fn variant(index: usize) -> WebSnapshot {
    let step = i64::try_from(index).unwrap();
    let mut snapshot = fixture();
    snapshot.generated_at += chrono::Duration::seconds(step);
    snapshot.tick_age_secs = Some(step % 5);
    let server = stage_mut(&mut snapshot, "server");
    server.elapsed_secs = Some(905 + step);
    server.execution_secs = Some(640 + step / 2);
    server.held = index.is_multiple_of(8);
    if index.is_multiple_of(3) {
        stage_mut(&mut snapshot, "merge-gate").merge_block = None;
    }
    snapshot.notice = index
        .is_multiple_of(4)
        .then(|| format!("notice {}", index % 3));
    if index.is_multiple_of(11) {
        snapshot.status.tokens = None;
    } else if let Some(tokens) = snapshot.status.tokens.as_mut() {
        tokens.observed_at += step / 5;
    }
    snapshot.status.progress.completed = index % 4;
    let design = stage_mut(&mut snapshot, "design");
    design.judge_heartbeat_secs = (index % 7 < 3).then_some(index as u64);
    // An attention entry carries its stage's heartbeat, as in collect_snapshot.
    let judge = (index % 9 < 4).then_some(index as u64);
    stage_mut(&mut snapshot, "docs").judge_heartbeat_secs = judge;
    let docs = snapshot.attention.iter_mut().find(|item| item.id == "docs");
    docs.expect("docs attention").judge_heartbeat_secs = judge;
    if index % 50 == 49 {
        snapshot.status.stages.pop();
    }
    snapshot
}

#[test]
fn reconstruct_equals_snapshot_over_a_scripted_sequence() {
    let mut harness = Harness::new();
    let mut fulls = 0;
    for index in 0..120 {
        let frames = harness.push(variant(index));
        fulls += kinds(&frames)
            .iter()
            .filter(|kind| **kind == "full")
            .count();
    }
    assert!(fulls >= 4, "stage removals and the 30s interval send fulls");
}

#[test]
fn a_subscriber_joining_mid_stream_reconstructs_the_final_snapshot() {
    const LAST: usize = 199;
    let broadcaster = Broadcaster::new();
    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..4)
        .map(|offset| {
            let broadcaster = broadcaster.clone();
            let done = Arc::clone(&done);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(30 * offset));
                follow(&broadcaster, &done)
            })
        })
        .collect();
    for index in 0..=LAST {
        broadcaster.publish(variant(index));
        thread::sleep(Duration::from_millis(1));
    }
    done.store(true, Ordering::SeqCst);
    let expected = serde_json::to_value(variant(LAST)).unwrap();
    for reader in readers {
        assert_eq!(reader.join().expect("reader thread"), expected);
    }
}

/// Behave like a client: reconstruct from the first `full`, reconnecting when
/// the hub drops the queue, until `done` is set and the stream has gone quiet.
fn follow(broadcaster: &Broadcaster, done: &AtomicBool) -> Value {
    let mut view = Value::Null;
    loop {
        let receiver = broadcaster.subscribe();
        let mut first = true;
        loop {
            match receiver.recv_timeout(Duration::from_millis(150)) {
                Ok(frame) => {
                    if std::mem::take(&mut first) {
                        assert!(frame.starts_with(r#"{"t":"full""#), "first frame is a full");
                    }
                    reconstruct(&mut view, &frame);
                }
                Err(RecvTimeoutError::Timeout) if done.load(Ordering::SeqCst) => {
                    // Everything was published before `done` was set.
                    while let Ok(frame) = receiver.try_recv() {
                        reconstruct(&mut view, &frame);
                    }
                    return view;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}
