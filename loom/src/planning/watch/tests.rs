use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::*;
use crate::planning::sections::normalized_hash;

const SUMMARY: &str = "doc/loom/summaries/plans/x.md";

fn tmp() -> (TempDir, PathBuf) {
    let temp = tempfile::Builder::new()
        .prefix("loom-watch-")
        .tempdir()
        .unwrap();
    let root = temp.path().to_path_buf();
    fs::create_dir_all(root.join("doc")).unwrap();
    (temp, root)
}

fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

fn p(rel: &str) -> PathBuf {
    PathBuf::from(rel)
}

fn started(root: &Path, own: &OwnWrites) -> Watcher {
    let mut watcher = Watcher::new(root.to_path_buf(), own.clone());
    assert!(watcher.poll().unwrap().unwrap().initial);
    watcher
}

/// Polls until a walk delivers (at most four walks); `None` when nothing was delivered.
fn deliver(watcher: &mut Watcher) -> Option<Batch> {
    (0..4).find_map(|_| watcher.poll().unwrap())
}

#[test]
fn initial_batch_lists_every_file_once() {
    let (_temp, root) = tmp();
    put(&root, "doc/b.md", "b\n");
    put(&root, "doc/plans/a.md", "a\n");
    put(&root, "doc/loom/summaries/s.md", "s\n");
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    let first = watcher.poll().unwrap().unwrap();
    let want = ["doc/b.md", "doc/loom/summaries/s.md", "doc/plans/a.md"].map(p);
    assert_eq!(
        first,
        Batch {
            added: want.to_vec(),
            initial: true,
            ..Batch::default()
        }
    );
    assert_eq!(deliver(&mut watcher), None);
}

#[test]
fn empty_doc_still_delivers_the_initial_batch() {
    let (_temp, root) = tmp();
    let mut watcher = Watcher::new(root, OwnWrites::default());
    let first = watcher.poll().unwrap().unwrap();
    assert!(first.initial && first.added.is_empty());
}

#[test]
fn binary_and_oversized_files_are_watched() {
    let (_temp, root) = tmp();
    let png = b"\x89PNG\r\n\x1a\n\xff\xfe\x00".to_vec();
    put(&root, "doc/images/logo.png", &png);
    put(&root, "doc/big.md", vec![b'a'; 9 * 1024 * 1024]);
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    let first = watcher.poll().unwrap().unwrap();
    assert_eq!(first.added, [p("doc/big.md"), p("doc/images/logo.png")]);
    put(&root, "doc/big.md", vec![b'b'; 9 * 1024 * 1024 + 1]);
    put(&root, "doc/images/logo.png", [&png[..], b"\x01"].concat());
    let batch = deliver(&mut watcher).unwrap();
    assert_eq!(batch.changed, [p("doc/big.md"), p("doc/images/logo.png")]);
}

#[test]
fn add_change_remove_in_one_window() {
    let (_temp, root) = tmp();
    put(&root, "doc/keep.md", "keep\n");
    put(&root, "doc/gone.md", "gone\n");
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/new.md", "new\n");
    put(&root, "doc/keep.md", "keep, longer now\n");
    fs::remove_file(root.join("doc/gone.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None, "changes found: wait");
    let want = Batch {
        added: vec![p("doc/new.md")],
        changed: vec![p("doc/keep.md")],
        removed: vec![p("doc/gone.md")],
        ..Batch::default()
    };
    assert_eq!(watcher.poll().unwrap(), Some(want));
}

#[test]
fn add_then_remove_cancels_and_change_after_add_stays_add() {
    let (_temp, root) = tmp();
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/brief.md", "brief\n");
    put(&root, "doc/kept.md", "kept\n");
    assert_eq!(watcher.poll().unwrap(), None);
    fs::remove_file(root.join("doc/brief.md")).unwrap();
    put(&root, "doc/kept.md", "kept, then edited\n");
    assert_eq!(watcher.poll().unwrap(), None);
    let batch = watcher.poll().unwrap().unwrap();
    assert_eq!(
        batch,
        Batch {
            added: vec![p("doc/kept.md")],
            ..Batch::default()
        }
    );
}

#[test]
fn rename_across_directories() {
    let (_temp, root) = tmp();
    put(&root, "doc/plans/PLAN-x.md", "# Plan x\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::create_dir_all(root.join("doc/plans/archive")).unwrap();
    let to = root.join("doc/plans/archive/DONE-PLAN-x.md");
    fs::rename(root.join("doc/plans/PLAN-x.md"), to).unwrap();
    let batch = deliver(&mut watcher).unwrap();
    let pair = (
        p("doc/plans/PLAN-x.md"),
        p("doc/plans/archive/DONE-PLAN-x.md"),
    );
    assert_eq!(
        batch,
        Batch {
            renamed: vec![pair],
            ..Batch::default()
        }
    );
}

#[test]
fn rename_then_edit_in_one_window_is_rename_plus_change() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    fs::rename(root.join("doc/a.md"), root.join("doc/b.md")).unwrap();
    assert_eq!(watcher.poll().unwrap(), None);
    put(&root, "doc/b.md", "alpha, edited after the move\n");
    assert_eq!(watcher.poll().unwrap(), None);
    let batch = watcher.poll().unwrap().unwrap();
    let want = Batch {
        changed: vec![p("doc/b.md")],
        renamed: vec![(p("doc/a.md"), p("doc/b.md"))],
        ..Batch::default()
    };
    assert_eq!(batch, want);
}

#[test]
fn touch_with_identical_content_reports_nothing() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = started(&root, &OwnWrites::default());
    put(&root, "doc/a.md", "alpha  \r\n");
    let file = fs::File::options()
        .write(true)
        .open(root.join("doc/a.md"))
        .unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000))
        .unwrap();
    assert_eq!(
        deliver(&mut watcher),
        None,
        "normalized content is unchanged"
    );
}

#[test]
fn temporaries_dotfiles_and_escaping_symlinks_are_never_reported() {
    let (temp, root) = tmp();
    let mut watcher = started(&root, &OwnWrites::default());
    for rel in [
        "doc/a.md~",
        "doc/a.md.swp",
        "doc/.#a.md",
        "doc/.hidden.md",
        "doc/.git/x.md",
    ] {
        put(&root, rel, "noise\n");
    }
    put(&root, "outside.md", "secret\n");
    symlink(temp.path().join("outside.md"), root.join("doc/escape.md")).unwrap();
    assert_eq!(deliver(&mut watcher), None);
}

#[test]
fn own_summary_write_is_suppressed_once() {
    let (_temp, root) = tmp();
    let own = OwnWrites::default();
    let mut watcher = started(&root, &own);
    let ours = "summary sealed by the engine\n";
    own.register(Path::new(SUMMARY), &normalized_hash(ours));
    put(&root, SUMMARY, ours);
    assert_eq!(deliver(&mut watcher), None, "own write");
    put(&root, SUMMARY, "summary edited by someone else\n");
    assert_eq!(deliver(&mut watcher).unwrap().changed, [p(SUMMARY)]);
    put(&root, SUMMARY, ours);
    assert_eq!(
        deliver(&mut watcher).unwrap().changed,
        [p(SUMMARY)],
        "registration consumed"
    );
}

#[test]
fn stale_registration_is_dropped_and_the_path_reported() {
    let (_temp, root) = tmp();
    let own = OwnWrites::default();
    let mut watcher = started(&root, &own);
    let registered = "what the engine meant to write\n";
    own.register(Path::new(SUMMARY), &normalized_hash(registered));
    put(&root, SUMMARY, "what someone else wrote\n");
    assert_eq!(deliver(&mut watcher).unwrap().added, [p(SUMMARY)]);
    put(&root, SUMMARY, registered);
    assert_eq!(
        deliver(&mut watcher).unwrap().changed,
        [p(SUMMARY)],
        "stale entry removed"
    );
}

#[test]
fn unregistered_write_and_paths_outside_the_store_are_reported() {
    let (_temp, root) = tmp();
    let own = OwnWrites::default();
    let mut watcher = started(&root, &own);
    let text = "same text\n";
    own.register(Path::new(SUMMARY), &normalized_hash(text));
    own.unregister(Path::new(SUMMARY));
    own.register(Path::new("doc/a.md"), &normalized_hash(text));
    put(&root, SUMMARY, text);
    put(&root, "doc/a.md", text);
    let batch = deliver(&mut watcher).unwrap();
    assert_eq!(batch.added, [p("doc/a.md"), p(SUMMARY)]);
}

#[test]
fn spawn_delivers_batches_and_stops_when_asked() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha\n");
    let watcher = Watcher::new(root.clone(), OwnWrites::default());
    let errors = watcher.error_slot();
    let stop = Arc::new(AtomicBool::new(false));
    let (send, batches) = mpsc::channel();
    let interval = Duration::from_millis(10);
    let handle = watcher
        .spawn(interval, Arc::clone(&stop), move |batch| {
            send.send(batch).unwrap()
        })
        .unwrap();
    assert!(
        batches
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .initial
    );
    put(&root, "doc/b.md", "bravo\n");
    let batch = batches.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(batch.added, [p("doc/b.md")]);
    stop.store(true, Ordering::Relaxed);
    let started = std::time::Instant::now();
    handle.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(*errors.lock().unwrap(), None);
}

#[test]
fn walk_errors_are_kept_and_cleared() {
    let (_temp, root) = tmp();
    let mut nested = String::from("doc");
    for _ in 0..20 {
        nested.push_str("/d");
    }
    put(&root, &format!("{nested}/deep.md"), "deep\n");
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    assert!(watcher.poll().is_err());
    assert!(watcher.last_error().is_some());
    fs::remove_dir_all(root.join("doc/d")).unwrap();
    assert!(watcher.poll().unwrap().unwrap().initial);
    assert_eq!(watcher.last_error(), None);
}
