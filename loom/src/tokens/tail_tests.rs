use super::{read_lines, stat};
use crate::tokens::tests_fixture::append;
use std::fs;
use tempfile::TempDir;

fn lines_from(path: &std::path::Path, offset: &mut u64) -> Vec<String> {
    let len = stat(path).expect("regular file").len;
    let mut lines = Vec::new();
    read_lines(path, offset, len, |line| lines.push(line.to_owned())).expect("read");
    lines
}

#[test]
fn partial_line_waits_for_its_newline() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("t.jsonl");
    fs::write(&path, "{\"a\":1}\n{\"b\":").expect("write");
    let mut offset = 0;

    assert_eq!(lines_from(&path, &mut offset), ["{\"a\":1}"]);
    assert_eq!(offset, 8, "the offset stays at the start of the torn line");
    assert!(lines_from(&path, &mut offset).is_empty());

    append(&path, "2}\n");
    assert_eq!(lines_from(&path, &mut offset), ["{\"b\":2}"]);
    assert_eq!(offset, 16);
}

#[test]
fn read_stops_at_the_stat_length() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("t.jsonl");
    fs::write(&path, "one\ntwo\n").expect("write");
    let mut offset = 0;
    let mut lines = Vec::new();

    read_lines(&path, &mut offset, 4, |line| lines.push(line.to_owned())).expect("read");

    assert_eq!(lines, ["one"]);
    assert_eq!(offset, 4);
}

#[test]
fn stat_refuses_missing_directory_and_fifo() {
    let tmp = TempDir::new().expect("tempdir");
    let fifo = tmp.path().join("pipe");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("mkfifo");

    assert_eq!(stat(&tmp.path().join("missing")), None);
    assert_eq!(stat(tmp.path()), None);
    assert_eq!(stat(&fifo), None);
}

#[test]
fn stat_reports_size_and_inode() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("t.jsonl");
    fs::write(&path, "abc\n").expect("write");
    let before = stat(&path).expect("stat");

    // Moving the old file aside keeps its inode in use, so the new one differs.
    fs::rename(&path, tmp.path().join("held")).expect("move aside");
    fs::write(&path, "abcdef\n").expect("rewrite");
    let after = stat(&path).expect("stat");

    assert_eq!(before.len, 4);
    assert_eq!(after.len, 7);
    assert_ne!(before.inode, after.inode, "a replaced file has a new inode");
}
