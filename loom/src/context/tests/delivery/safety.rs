use super::*;
use crate::context::delivery::DELIVERY_RECORD_MAX_BYTES;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::{mpsc, Arc, Barrier};
use std::time::Duration;

#[test]
fn special_and_oversized_records_are_skipped_and_fifo_update_completes() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join("work");
    let good = record("good", "epoch", &[("safe", "hash")]);
    record_delivery(&work, PLAN, STAGE, &good).unwrap();
    let dir = delivery_dir(&work, PLAN, STAGE);
    let outside = temp.path().join("outside.json");
    fs::write(
        &outside,
        serde_json::to_vec(&record("outside", "epoch", &[("secret", "hash")])).unwrap(),
    )
    .unwrap();
    symlink(&outside, dir.join("link.json")).unwrap();
    fs::write(
        dir.join("large.json"),
        vec![b'x'; DELIVERY_RECORD_MAX_BYTES + 1],
    )
    .unwrap();
    nix::unistd::mkfifo(&dir.join("fifo.json"), nix::sys::stat::Mode::S_IRWXU).unwrap();

    let (tx, rx) = mpsc::channel();
    let work_clone = work.clone();
    std::thread::spawn(move || {
        let loaded = load_deliveries(&work_clone, PLAN, STAGE).unwrap();
        let update = record_delivery(&work_clone, PLAN, STAGE, &record("fifo", "epoch", &[]));
        tx.send((loaded, update.is_ok())).unwrap();
    });
    let (loaded, updated) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("FIFO blocked delivery");
    assert_eq!(loaded, vec![good]);
    assert!(updated);
    assert!(record_delivery(&work, PLAN, STAGE, &record("large", "epoch", &[])).is_err());
    assert_eq!(
        fs::metadata(dir.join("large.json")).unwrap().len(),
        (DELIVERY_RECORD_MAX_BYTES + 1) as u64
    );
    assert_symlink_record_replaced(&work, &outside);
}

fn assert_symlink_record_replaced(work: &Path, outside: &Path) {
    record_delivery(
        work,
        PLAN,
        STAGE,
        &record("link", "epoch", &[("new", "hash")]),
    )
    .unwrap();
    let linked = load_deliveries(work, PLAN, STAGE)
        .unwrap()
        .into_iter()
        .find(|entry| entry.recipient_id == "link")
        .unwrap();
    assert_eq!(linked.delivered.len(), 1);
    assert_eq!(linked.delivered[0].node_id, "new");
    assert!(fs::read_to_string(outside).unwrap().contains("outside"));
}

#[test]
fn intermediate_symlinks_cannot_supply_records() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join("work");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    record_delivery(&outside, PLAN, STAGE, &record("external", "epoch", &[])).unwrap();
    fs::create_dir_all(&work).unwrap();
    symlink(outside.join("context"), work.join("context")).unwrap();
    assert!(load_deliveries(&work, PLAN, STAGE).unwrap().is_empty());
    let original = fs::read(delivery_dir(&outside, PLAN, STAGE).join("external.json")).unwrap();
    assert!(record_delivery(
        &work,
        PLAN,
        STAGE,
        &record("external", "epoch", &[("new", "hash")])
    )
    .is_err());
    assert_eq!(
        fs::read(delivery_dir(&outside, PLAN, STAGE).join("external.json")).unwrap(),
        original
    );
}

#[test]
fn oversized_union_preserves_existing_record() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join("work");
    let large = "a".repeat(DELIVERY_RECORD_MAX_BYTES / 2);
    let first = record("session", "epoch", &[("first", &large)]);
    let second = record("session", "epoch", &[("second", &large)]);
    record_delivery(&work, PLAN, STAGE, &first).unwrap();
    assert!(record_delivery(&work, PLAN, STAGE, &second).is_err());
    assert_eq!(load_deliveries(&work, PLAN, STAGE).unwrap(), vec![first]);
}

#[test]
fn concurrent_updates_keep_both_deliveries() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join("work");
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|id| {
            let work = work.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                record_delivery(
                    &work,
                    PLAN,
                    STAGE,
                    &record("session", "epoch", &[(id, "hash")]),
                )
                .unwrap();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let records = load_deliveries(&work, PLAN, STAGE).unwrap();
    let ids: Vec<_> = records[0]
        .delivered
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    assert_eq!(ids, ["first", "second"]);
}
