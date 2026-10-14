use super::{next_line, read_lines, stat, Line, MAX_LINE_BYTES};
use crate::tokens::tests_fixture::append;
use std::fs;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::TempDir;

/// The text of a line, or `<too long>` for a skipped one.
fn text(line: Line<'_>) -> String {
    match line {
        Line::Text(text) => text.to_owned(),
        Line::TooLong => "<too long>".to_owned(),
    }
}

fn lines_from(path: &std::path::Path, offset: &mut u64) -> Vec<String> {
    let len = stat(path).expect("regular file").len;
    let mut lines = Vec::new();
    read_lines(path, offset, len, &AtomicBool::new(false), |line| {
        lines.push(text(line));
    })
    .expect("read");
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

    read_lines(&path, &mut offset, 4, &AtomicBool::new(false), |line| {
        lines.push(text(line));
    })
    .expect("read");

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

#[test]
fn stat_and_read_refuse_a_symlinked_file() {
    let tmp = TempDir::new().expect("tempdir");
    let target = tmp.path().join("target.jsonl");
    let link = tmp.path().join("link.jsonl");
    fs::write(&target, "one\n").expect("write");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    assert_eq!(stat(&link), None);
    let mut offset = 0;
    let read = read_lines(&link, &mut offset, 4, &AtomicBool::new(false), |_| {});
    assert!(read.is_err(), "O_NOFOLLOW must refuse the link");
    assert_eq!(offset, 0);
}

/// `next_line` with a cap of 4 bytes.
fn next(reader: &mut Cursor<Vec<u8>>, line: &mut Vec<u8>) -> Option<(u64, bool)> {
    next_line(reader, line, 4).expect("read")
}

#[test]
fn next_line_cuts_a_long_line_and_still_consumes_it() {
    let mut reader = Cursor::new(b"ok\n0123456789\nnext\n".to_vec());
    let mut line = Vec::new();

    assert_eq!(next(&mut reader, &mut line), Some((3, false)));
    assert_eq!(line, b"ok");
    assert_eq!(next(&mut reader, &mut line), Some((11, true)));
    assert!(line.len() <= 4, "a long line is never buffered whole");
    assert_eq!(next(&mut reader, &mut line), Some((5, false)));
    assert_eq!(line, b"next");
    assert_eq!(next(&mut reader, &mut line), None);
}

#[test]
fn next_line_keeps_a_line_exactly_at_the_cap() {
    let mut reader = Cursor::new(b"abcd\nabcde\n".to_vec());
    let mut line = Vec::new();

    assert_eq!(next(&mut reader, &mut line), Some((5, false)));
    assert_eq!(line, b"abcd");
    assert_eq!(next(&mut reader, &mut line), Some((6, true)));
}

#[test]
fn next_line_reports_an_unfinished_line_as_none() {
    let mut reader = Cursor::new(b"torn".to_vec());

    assert_eq!(next(&mut reader, &mut Vec::new()), None);
}

#[test]
fn overlong_line_is_skipped_counted_and_passed() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("t.jsonl");
    let mut body = b"before\n".to_vec();
    body.resize(body.len() + MAX_LINE_BYTES + 10, b'x');
    body.extend_from_slice(b"\nafter\n");
    fs::write(&path, &body).expect("write");
    let mut offset = 0;

    assert_eq!(
        lines_from(&path, &mut offset),
        ["before", "<too long>", "after"]
    );
    assert_eq!(offset, body.len() as u64, "the long line is advanced past");
}

#[test]
fn read_stops_between_lines_once_the_flag_is_set() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("t.jsonl");
    fs::write(&path, "one\ntwo\nthree\n").expect("write");
    let stop = AtomicBool::new(false);
    let mut offset = 0;
    let mut seen = Vec::new();

    read_lines(&path, &mut offset, 14, &stop, |line| {
        seen.push(text(line));
        stop.store(true, Ordering::SeqCst);
    })
    .expect("read");

    assert_eq!(seen, ["one"]);
    assert_eq!(
        offset, 4,
        "the offset stays after the last line handed over"
    );

    let mut untouched = 0;
    read_lines(&path, &mut untouched, 14, &stop, |_| panic!("stopped")).expect("read");
    assert_eq!(untouched, 0);
}
