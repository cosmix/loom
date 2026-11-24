use std::ffi::CString;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::tempdir;

use super::super::codex_provider;
use super::super::discovery::DiscoveredFile;
use super::super::time_range::TimeRange;
use super::super::transcript::{self, Scope};
use super::{for_each_line, open_regular, MAX_LINE_BYTES};

fn fifo_in(directory: &Path) -> PathBuf {
    let path = directory.join("fifo.jsonl");
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    path
}

fn within_watchdog<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("FIFO scan did not return a result")
}

fn range() -> TimeRange {
    TimeRange {
        since: "2026-09-01T00:00:00Z".parse().unwrap(),
        until: None,
    }
}

fn collect_lines(path: &Path) -> Vec<(usize, Option<String>)> {
    let mut lines = Vec::new();
    for_each_line(open_regular(path).unwrap(), |ordinal, line| {
        lines.push((ordinal, line.map(str::to_owned)));
    })
    .unwrap();
    lines
}

fn write_padded_rows(path: &Path, mut first: Value, second: Value) {
    let mut file = File::create(path).unwrap();
    first["pad"] = Value::String("x".repeat(MAX_LINE_BYTES));
    serde_json::to_writer(&mut file, &first).unwrap();
    writeln!(file).unwrap();
    serde_json::to_writer(&mut file, &second).unwrap();
}

#[test]
fn a_fifo_is_refused_without_blocking() {
    let directory = tempdir().unwrap();
    let path = fifo_in(directory.path());
    let err = within_watchdog(move || open_regular(&path)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn lines_are_delivered_with_ordinals_and_an_unterminated_last_line() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("lines.jsonl");
    fs::write(&path, b"a\r\nb\n\xff\nlast").unwrap();
    assert_eq!(
        collect_lines(&path),
        [
            (0, Some("a".to_owned())),
            (1, Some("b".to_owned())),
            (2, None),
            (3, Some("last".to_owned())),
        ]
    );
}

#[test]
fn an_overlong_line_is_cut_and_the_next_row_still_reads() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("lines.jsonl");
    let mut bytes = vec![b'x'; MAX_LINE_BYTES + 10];
    bytes.extend_from_slice(b"\nshort\n");
    fs::write(&path, bytes).unwrap();

    assert_eq!(collect_lines(&path), [(0, None), (1, Some("short".into()))]);
}

#[test]
fn transcript_parse_counts_an_overlong_valid_row_as_malformed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("claude.jsonl");
    let row = |id| {
        json!({
            "type": "assistant",
            "timestamp": "2026-09-12T20:00:00Z",
            "message": {"id": id, "model": "x", "content": [], "usage": {"input_tokens": 1}},
        })
    };
    write_padded_rows(&path, row("m"), row("normal"));
    let file = DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "session".to_owned(),
        agent_id: None,
    };

    let transcript = transcript::parse(&file, &range()).unwrap();
    assert_eq!(transcript.diagnostics.malformed_rows, 1);
    let ids: Vec<_> = transcript
        .requests()
        .map(|r| r.message_id.as_deref())
        .collect();
    assert_eq!(ids, [Some("normal")]);
}

#[test]
fn codex_normalize_counts_a_fifo_as_unreadable_and_an_overlong_row_as_malformed() {
    let directory = tempdir().unwrap();
    let fifo = fifo_in(directory.path());
    let result = within_watchdog(move || codex_provider::normalize(&[fifo], &range()));
    assert_eq!(result.diagnostics.unreadable_files, 1);

    let path = directory.path().join("rollout.jsonl");
    let row = |id| {
        json!({
            "type": "token_usage_record",
            "timestamp": "2026-09-12T20:00:00Z",
            "payload": {
                "response_id": id,
                "model": "gpt-codex",
                "usage": {
                    "input_tokens": 1,
                    "cached_input_tokens": 0,
                    "output_tokens": 2,
                    "reasoning_output_tokens": 0,
                    "total_tokens": 3,
                },
            },
        })
    };
    write_padded_rows(&path, row("padded"), row("normal"));
    let result = codex_provider::normalize(&[path], &range());
    assert_eq!(result.diagnostics.malformed_rows, 1);
    let ids: Vec<_> = result
        .rows
        .iter()
        .map(|r| r.request_id.as_deref())
        .collect();
    assert_eq!(ids, [Some("normal")]);
}
