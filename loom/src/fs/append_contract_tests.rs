//! Contracts for locked no-follow appends, the complete-line JSONL reader and the
//! no-follow regular-file predicate (plan Decision A7 and the fs-safety public surface).

use std::os::unix::fs::symlink;
use std::path::Path;

use fs2::FileExt;

use crate::fs::jsonl_spool::{append, read_complete_lines};
use crate::fs::safe_fs::safe_open_append;
use crate::fs::safe_read::is_regular_no_follow;
use crate::hooks::{events::log_hook_event, HookEvent, HookEventLog};

#[test]
fn spool_append_refuses_symlink() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("target.jsonl");
    std::fs::write(&target, "x\n").unwrap();
    symlink(&target, root.path().join("spool.jsonl")).unwrap();

    let result = append(
        root.path(),
        "spool.jsonl",
        &serde_json::json!({"a": 1}),
        4096,
    );

    assert!(result.is_err(), "append followed a planted symlink");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "x\n");
}

#[test]
fn hook_events_append_refuses_symlink() {
    let work_dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("outside.jsonl");
    std::fs::write(&target, "untouched\n").unwrap();
    let hooks = work_dir.path().join("hooks");
    std::fs::create_dir(&hooks).unwrap();
    symlink(&target, hooks.join("events.jsonl")).unwrap();

    let event = HookEventLog::new("stage", "session", HookEvent::Stop);
    let result = log_hook_event(work_dir.path(), event);

    assert!(result.is_err(), "hook event log followed a planted symlink");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched\n");
}

#[test]
fn safe_open_append_holds_lock() {
    let dir = tempfile::tempdir().unwrap();
    let held = safe_open_append(dir.path(), Path::new("log")).unwrap();

    let second = std::fs::File::open(dir.path().join("log")).unwrap();
    assert!(
        second.try_lock_exclusive().is_err(),
        "safe_open_append returned a file without holding its exclusive lock"
    );

    drop(held);
    second
        .try_lock_exclusive()
        .expect("the lock is released when the appender's File drops");
}

#[test]
fn complete_lines_skip_torn_tail_and_blank() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.jsonl");
    std::fs::write(&path, "{\"a\":1}\n\n{\"b\":2}\n{\"c\"").unwrap();

    let lines = read_complete_lines(&path, 1024, 10).unwrap();

    assert_eq!(
        lines,
        vec![b"{\"a\":1}".to_vec(), b"{\"b\":2}".to_vec()],
        "expected exactly the two complete, non-empty lines"
    );
}

#[test]
fn complete_lines_refuse_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.jsonl");
    std::fs::write(&target, "{\"a\":1}\n").unwrap();
    let link = dir.path().join("link.jsonl");
    symlink(&target, &link).unwrap();

    let result = read_complete_lines(&link, 1024, 10);

    assert!(
        result.is_err(),
        "read_complete_lines followed a symlink: {result:?}"
    );
}

#[test]
fn regular_predicate_does_not_follow() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, "data").unwrap();
    let link = dir.path().join("l");
    symlink(&file, &link).unwrap();

    assert!(is_regular_no_follow(&file), "a regular file is regular");
    assert!(
        !is_regular_no_follow(&link),
        "a symlink to a regular file must not count as regular"
    );
}
