//! Walk-comparison and pending-batch merge cases of the watcher.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime};

use super::tests::{deliver, p, put, started, tmp};
use super::*;

/// Set `rel`'s modification time to `secs` after the epoch.
fn set_mtime(root: &Path, rel: &str, secs: u64) {
    let file = fs::File::options()
        .write(true)
        .open(root.join(rel))
        .unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

/// One batch holding only `changed`.
fn changed_only(paths: &[&str]) -> Batch {
    Batch {
        changed: paths.iter().map(|rel| p(rel)).collect(),
        ..Batch::default()
    }
}

#[test]
fn same_size_edit_is_seen_through_the_modification_time() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    set_mtime(&root, "doc/a.md", 1_000_000);
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/a.md", "bravo\n");
    set_mtime(&root, "doc/a.md", 2_000_000);
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn same_size_same_mtime_replacement_is_seen_through_the_inode() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    set_mtime(&root, "doc/a.md", 1_000_000);
    let mut watcher = started(&root, &OwnWrites::default());
    // Written outside doc/ so the walk never sees the staging file, then moved over `a.md`.
    put(&root, "staging.md", "bravo\n");
    set_mtime(&root, "staging.md", 1_000_000);
    fs::rename(root.join("staging.md"), root.join("doc/a.md")).unwrap();
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn same_mtime_same_inode_edit_is_seen_through_the_size() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    set_mtime(&root, "doc/a.md", 1_000_000);
    let mut watcher = started(&root, &OwnWrites::default());
    // `put` truncates and rewrites the existing file, so the inode stays; with the original
    // modification time restored, the longer text is the only difference the walk can see.
    put(&root, "doc/a.md", "alpha, rewritten with a longer text\n");
    set_mtime(&root, "doc/a.md", 1_000_000);
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn removed_then_recreated_across_walks_is_a_change() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::remove_file(root.join("doc/a.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None, "removal found: wait");
    put(&root, "doc/a.md", "alpha, written again with other text\n");
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn removing_a_pending_change_reports_only_the_removal() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/a.md", "alpha, edited\n");
    assert_eq!(watcher.poll().unwrap(), None);
    fs::remove_file(root.join("doc/a.md")).unwrap();
    let want = Batch {
        removed: vec![p("doc/a.md")],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn renaming_a_file_back_to_its_origin_leaves_nothing_pending() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    fs::rename(root.join("doc/b.md"), root.join("doc/a.md")).unwrap();
    assert_eq!(
        deliver(&mut watcher),
        None,
        "the round trip reports nothing"
    );
    // A stale `a -> a` rename left pending would surface in the next delivery.
    put(&root, "doc/a.md", "alpha, edited\n");
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn rename_chain_reports_the_first_and_last_path() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    fs::rename(root.join("doc/b.md"), root.join("doc/c.md")).unwrap();
    let want = Batch {
        renamed: vec![(p("doc/a.md"), p("doc/c.md"))],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn renamed_pending_add_is_an_add_at_the_new_path() {
    let (_temp, root) = tmp();
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/n.md", "new\n");
    assert_eq!(watcher.poll().unwrap(), None);
    fs::rename(root.join("doc/n.md"), root.join("doc/m.md")).unwrap();
    let want = Batch {
        added: vec![p("doc/m.md")],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn renamed_pending_change_follows_the_file() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/a.md", "alpha, edited\n");
    assert_eq!(watcher.poll().unwrap(), None);
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    let want = Batch {
        changed: vec![p("doc/b.md")],
        renamed: vec![(p("doc/a.md"), p("doc/b.md"))],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn rename_over_a_removed_path_is_a_rename_plus_change() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    put(&root, "doc/b.md", "bravo\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::remove_file(root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    let want = Batch {
        changed: vec![p("doc/b.md")],
        renamed: vec![(p("doc/a.md"), p("doc/b.md"))],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn removing_a_renamed_file_removes_its_original_path() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    fs::remove_file(root.join("doc/b.md")).unwrap();
    let want = Batch {
        removed: vec![p("doc/a.md")],
        ..Batch::default()
    };
    assert_eq!(deliver(&mut watcher), Some(want));
}

#[test]
fn removing_a_renamed_file_whose_origin_was_recreated_is_a_change_of_the_origin() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    put(&root, "doc/a.md", "a different file at the old path\n");
    assert_eq!(watcher.poll().unwrap(), None);
    fs::remove_file(root.join("doc/b.md")).unwrap();
    assert_eq!(deliver(&mut watcher), Some(changed_only(&["doc/a.md"])));
}

#[test]
fn failed_walks_deliver_no_batch_and_stay_in_the_error_slot_until_a_walk_succeeds() {
    let (_temp, root) = tmp();
    let mut nested = String::from("doc");
    for _ in 0..20 {
        nested.push_str("/d");
    }
    put(&root, &format!("{nested}/deep.md"), "deep\n");
    let watcher = Watcher::new(root.clone(), OwnWrites::default());
    let errors = watcher.error_slot();
    let stop = Arc::new(AtomicBool::new(false));
    let (send, batches) = mpsc::channel();
    let handle = watcher
        .spawn(Duration::from_millis(10), Arc::clone(&stop), move |batch| {
            send.send(batch).unwrap()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while errors.lock().unwrap().is_none() {
        assert!(Instant::now() < deadline, "no walk failure was recorded");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        batches.try_recv().is_err(),
        "a failed walk delivers nothing"
    );
    fs::remove_dir_all(root.join("doc/d")).unwrap();
    let first = batches.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(first.initial);
    assert_eq!(*errors.lock().unwrap(), None, "a good walk clears the slot");
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}
