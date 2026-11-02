use super::*;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

#[test]
fn save_waits_for_plan_writer_then_rechecks_the_opened_hash() {
    if skip("save_waits_for_plan_writer_then_rechecks_the_opened_hash") {
        return;
    }
    let original = plan(".", "true", "");
    let (dir, server) = served(&[("doc/plans/PLAN-x.md", &original)], writable());
    let path = dir.path().join("doc/plans/PLAN-x.md");
    let lock = crate::fs::locking::lock_plan(&path).unwrap();
    let (started_tx, started_rx) = channel();
    let (done_tx, done_rx) = channel();
    let ctx = server.ctx.clone();
    let opened = original.clone();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = save_document(
            &ctx,
            &BrowserId::mint().unwrap(),
            SaveRequest {
                path: "plans/PLAN-x.md",
                text: &opened,
                opened_hash: &normalized_hash(&opened),
                force: false,
            },
        );
        done_tx.send(result).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let waiting = done_rx.recv_timeout(Duration::from_millis(100));
    let updated = crate::pressure::record::append_to_text(&original, "- newer review");
    std::fs::write(&path, &updated).unwrap();
    drop(lock);
    assert!(
        matches!(waiting, Err(RecvTimeoutError::Timeout)),
        "{waiting:?}"
    );
    assert_eq!(
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        Ok(SaveOutcome::Conflict {
            current_hash: normalized_hash(&updated),
            current_text: updated.clone(),
        })
    );
    worker.join().unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), updated);
    server.stop().unwrap();
}
