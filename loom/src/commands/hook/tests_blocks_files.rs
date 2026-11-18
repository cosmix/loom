//! Ledger files `loom hook blocks` cannot open, and the bounded tail's first line.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

use tempfile::TempDir;

use super::blocks::{summarize, Summary};
use super::tests_blocks::{line, now, write_ledger};

fn subjects(report: &Summary) -> Vec<&str> {
    report.records.iter().map(|r| r.subject.as_str()).collect()
}

/// A Unix socket at a ledger path fails open (ENXIO on Linux, EOPNOTSUPP on
/// macOS); that ledger is skipped and the others are still reported.
#[test]
fn socket_ledger_is_skipped_and_others_are_reported() {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("u.blocks.jsonl");
    // A sandbox may refuse bind (EPERM); mknod makes the same socket inode.
    let listener = UnixListener::bind(&socket).ok();
    if listener.is_none() {
        let mode = nix::sys::stat::Mode::S_IRWXU;
        nix::sys::stat::mknod(&socket, nix::sys::stat::SFlag::S_IFSOCK, mode, 0).unwrap();
    }
    write_ledger(
        dir.path(),
        "s",
        &[line("2026-11-07T11:00:00Z", "s", "kept")],
    );
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert_eq!(subjects(&report), ["kept"]);
}

/// A mode-000 ledger fails open with EACCES; it is skipped, not fatal.
#[test]
fn unreadable_ledger_is_skipped_and_others_are_reported() {
    let dir = TempDir::new().unwrap();
    let locked = dir.path().join("l.blocks.jsonl");
    fs::write(&locked, line("2026-11-07T11:00:00Z", "l", "hidden") + "\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&locked).is_ok() {
        // root (or CAP_DAC_OVERRIDE) opens it anyway; nothing to prove here.
        return;
    }
    write_ledger(
        dir.path(),
        "s",
        &[line("2026-11-07T11:00:00Z", "s", "kept")],
    );
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert_eq!(subjects(&report), ["kept"]);
}

/// When the 4 MiB window starts exactly on a record, that record is kept.
#[test]
fn oversized_file_keeps_a_window_that_starts_on_a_line_boundary() {
    let dir = TempDir::new().unwrap();
    let older = line("2026-11-07T10:00:00Z", "s", "older") + "\n";
    let newest = line("2026-11-07T11:00:00Z", "s", "newest") + "\n";
    let mut bytes = b"prefix\n".to_vec();
    bytes.extend_from_slice(older.as_bytes());
    // Malformed filler lines of at most 4,001 bytes fill the window exactly.
    let mut filler = 4 * 1024 * 1024 - older.len() - newest.len();
    let mut fillers = 0;
    while filler > 0 {
        let take = filler.min(4001);
        bytes.extend(vec![b'x'; take - 1]);
        bytes.push(b'\n');
        filler -= take;
        fillers += 1;
    }
    bytes.extend_from_slice(newest.as_bytes());
    fs::write(dir.path().join("s.blocks.jsonl"), bytes).unwrap();
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert_eq!(subjects(&report), ["newest", "older"]);
    assert_eq!(report.malformed, fillers);
}
