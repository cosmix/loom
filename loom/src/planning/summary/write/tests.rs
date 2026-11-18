//! Store writes: temp-and-rename, crashed temps, own-write registration, the conditional
//! write and symlink refusal.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use super::*;
use crate::planning::summary::evaluate::fixtures::{fixture, put, two_sections, Fixture, ID1};
use crate::planning::watch::{Batch, Watcher};

const TEXT: &str = "summary text\n";
const ID2_SIBLING: &str = "demo/product/section_02";

fn on_disk(fx: &Fixture, id: &str) -> Option<String> {
    fs::read_to_string(fx.root.join(store::summary_path(id))).ok()
}

fn poll_batch(watcher: &mut Watcher) -> Batch {
    (0..10)
        .find_map(|_| watcher.poll().expect("poll"))
        .expect("the watcher delivered no batch")
}

#[test]
fn removes_a_crashed_temp_of_another_pid_before_writing() {
    let fx = two_sections();
    let stale = temp_path(ID1, std::process::id().wrapping_add(7));
    let other = store::summary_path(ID2_SIBLING).with_file_name(".section_02.md.tmp-9");
    put(&fx.root, &stale.to_string_lossy(), "half written");
    put(&fx.root, &other.to_string_lossy(), "another target's temp");
    write_summary(&fx.root, &fx.paths, ID1, TEXT, None).expect("write");
    assert_eq!(on_disk(&fx, ID1).as_deref(), Some(TEXT));
    assert!(!fx.root.join(&stale).exists(), "crashed temp kept");
    assert!(
        !fx.root.join(temp_path(ID1, std::process::id())).exists(),
        "own temp kept"
    );
    assert!(
        fx.root.join(&other).exists(),
        "another target's temp removed"
    );
}

#[test]
fn registered_write_is_not_reported_and_failed_write_unregisters() {
    let fx = two_sections();
    let own = OwnWrites::default();
    let mut watcher = Watcher::new(fx.root.clone(), own.clone());
    poll_batch(&mut watcher);
    write_summary(&fx.root, &fx.paths, ID1, TEXT, Some(&own)).expect("write");
    // An outside edit in the same window, so the walk has something to deliver.
    put(&fx.root, "doc/notes.md", "outside edit\n");
    let rel = store::summary_path(ID1);
    let delivered = poll_batch(&mut watcher);
    assert_eq!(
        delivered.added,
        vec![std::path::PathBuf::from("doc/notes.md")],
        "{delivered:?}"
    );
    assert!(
        !delivered.added.contains(&rel),
        "own write reported: {delivered:?}"
    );
    // A non-empty directory in the target's place makes the rename fail.
    let blocked = store::summary_path(ID2_SIBLING);
    put(&fx.root, &blocked.join("child").to_string_lossy(), "x");
    poll_batch(&mut watcher);
    let failed = write_summary(&fx.root, &fx.paths, ID2_SIBLING, TEXT, Some(&own));
    assert!(failed.is_err(), "rename over a directory succeeded");
    fs::remove_dir_all(fx.root.join(&blocked)).unwrap();
    put(&fx.root, &blocked.to_string_lossy(), TEXT);
    put(&fx.root, "doc/notes.md", "second outside edit\n");
    let delivered = poll_batch(&mut watcher);
    let reported = delivered.added.contains(&blocked) || delivered.changed.contains(&blocked);
    assert!(
        reported,
        "a failed write left its registration: {delivered:?}"
    );
}

#[test]
fn conditional_write_skips_when_the_file_changed() {
    let fx = two_sections();
    let built = |text: &'static str| move || Ok(text.to_string());
    let first =
        write_summary_if(&fx.root, &fx.paths, ID1, None, built(TEXT), None).expect("create");
    assert_eq!(first, WriteOutcome::Written);
    let again = write_summary_if(&fx.root, &fx.paths, ID1, None, built("other\n"), None);
    assert_eq!(again.expect("absent expected"), WriteOutcome::Changed);
    let stale_hash = normalized_hash("what the caller read\n");
    let outcome = write_summary_if(
        &fx.root,
        &fx.paths,
        ID1,
        Some(&stale_hash),
        built("x\n"),
        None,
    );
    assert_eq!(outcome.expect("changed"), WriteOutcome::Changed);
    assert_eq!(
        on_disk(&fx, ID1).as_deref(),
        Some(TEXT),
        "a changed file was overwritten"
    );
    let current = normalized_hash(TEXT);
    let outcome = write_summary_if(
        &fx.root,
        &fx.paths,
        ID1,
        Some(&current),
        built("new\n"),
        None,
    );
    assert_eq!(outcome.expect("matching"), WriteOutcome::Written);
    assert_eq!(on_disk(&fx, ID1).as_deref(), Some("new\n"));
}

#[test]
fn symlinked_store_directory_is_refused() {
    let fx = two_sections();
    let elsewhere = fx.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(fx.root.join("doc/loom")).unwrap();
    symlink(&elsewhere, fx.root.join("doc/loom/summaries")).unwrap();
    assert!(write_summary(&fx.root, &fx.paths, ID1, TEXT, None).is_err());
    assert!(
        fs::read_dir(&elsewhere).unwrap().next().is_none(),
        "wrote through the link"
    );
    assert!(
        read_summary(&fx.root, ID1).is_err(),
        "read through the link"
    );
    assert!(stored_ids(&fx.root).is_err(), "listed through the link");
}

#[test]
fn symlinked_summary_file_is_not_read() {
    let fx = two_sections();
    put(&fx.root, "doc/elsewhere.md", TEXT);
    fs::create_dir_all(fx.root.join("doc/loom/summaries/demo/product")).unwrap();
    symlink(
        fx.root.join("doc/elsewhere.md"),
        fx.root.join(store::summary_path(ID1)),
    )
    .unwrap();
    assert!(read_summary(&fx.root, ID1).is_err());
    assert_eq!(read_summary(&fx.root, ID2_SIBLING).expect("absent"), None);
}

#[test]
fn delete_removes_the_file_and_tolerates_absence() {
    let fx = two_sections();
    write_summary(&fx.root, &fx.paths, ID1, TEXT, None).expect("write");
    delete_summary(&fx.root, &fx.paths, ID1).expect("delete");
    assert_eq!(on_disk(&fx, ID1), None);
    delete_summary(&fx.root, &fx.paths, ID1).expect("absent is fine");
    assert!(
        delete_summary(&fx.root, &fx.paths, "../escape").is_err(),
        "invalid id"
    );
}

#[test]
fn store_lock_creates_runtime_dir_and_refuses_a_symlinked_lock() {
    let fx = fixture(&[]);
    assert!(!fx.paths.dir.exists());
    let value = with_store_lock(&fx.paths, || Ok(7)).expect("lock");
    assert_eq!(value, 7);
    let mode = fs::metadata(&fx.paths.dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    let lock_mode = fs::metadata(&fx.paths.summaries_lock)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(lock_mode, 0o600);
    let failed: Result<()> = with_store_lock(&fx.paths, || Err(anyhow!("inner")));
    assert!(failed.is_err());
    with_store_lock(&fx.paths, || Ok(())).expect("released after an error");
    fs::remove_file(&fx.paths.summaries_lock).unwrap();
    symlink(fx.root.join("target"), &fx.paths.summaries_lock).unwrap();
    assert!(
        with_store_lock(&fx.paths, || Ok(())).is_err(),
        "followed a symlinked lock"
    );
    assert!(!fx.root.join("target").exists());
}

#[test]
fn lists_nested_sections_and_overviews_and_skips_unsafe_directories() {
    let fx = fixture(&[]);
    let expected = [
        ID1,
        "other/product/section_01",
        "overview/demo/product",
        "overview/other/product",
        "plans/x",
    ];
    for id in expected {
        write_summary(&fx.root, &fx.paths, id, TEXT, None).expect("write nested id");
    }
    put(
        &fx.root,
        "doc/loom/summaries/overview/demo/.product.md.tmp-1",
        TEXT,
    );
    put(
        &fx.root,
        "doc/loom/summaries/demo/product/section_1.md",
        TEXT,
    );
    put(
        &fx.root,
        "doc/loom/summaries/.hidden/product/section_01.md",
        TEXT,
    );
    let outside = fx.root.join("outside");
    fs::create_dir_all(outside.join("product")).unwrap();
    fs::write(outside.join("product/section_01.md"), TEXT).unwrap();
    symlink(&outside, fx.root.join("doc/loom/summaries/linked")).unwrap();

    assert_eq!(stored_ids(&fx.root).expect("list"), expected);
}

#[test]
fn nested_store_parent_symlink_refuses_writes_and_reads() {
    let fx = fixture(&[]);
    let outside = fx.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(fx.root.join("doc/loom/summaries/demo")).unwrap();
    symlink(&outside, fx.root.join("doc/loom/summaries/demo/product")).unwrap();

    assert!(write_summary(&fx.root, &fx.paths, ID1, TEXT, None).is_err());
    assert!(read_summary(&fx.root, ID1).is_err());
    assert!(fs::read_dir(&outside).unwrap().next().is_none());
}
