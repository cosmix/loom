//! Contract: the usage scan never blocks on a FIFO and never parses a line
//! over `MAX_LINE_BYTES`; such a line is one malformed row, not a request.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::discovery::DiscoveredFile;
use super::time_range::TimeRange;
use super::transcript::{self, Scope};
use crate::tokens::tail::MAX_LINE_BYTES;

const WATCHDOG: Duration = Duration::from_secs(10);

fn range() -> TimeRange {
    TimeRange {
        since: Utc.with_ymd_and_hms(2026, 9, 12, 0, 0, 0).unwrap(),
        until: None,
    }
}

fn fifo_in(dir: &Path) -> PathBuf {
    let path = dir.join("x.jsonl");
    let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    path
}

fn discovered(path: PathBuf) -> DiscoveredFile {
    DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "session".to_owned(),
        agent_id: None,
    }
}

/// Runs `work` on a thread; a result later than the watchdog is `None`. A
/// thread blocked on a FIFO is leaked on purpose: nothing can unblock it.
fn within_watchdog<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(WATCHDOG).ok()
}

fn claude_row(id: &str, output: u64, pad: &str) -> String {
    json!({
        "type": "assistant",
        "timestamp": "2026-09-12T20:00:00Z",
        "message": {
            "id": id,
            "model": "model",
            "content": [{"type": "text", "text": pad}],
            "usage": {"input_tokens": 1, "output_tokens": output},
        },
    })
    .to_string()
}

fn codex_row(id: &str, output: u64, pad: &str) -> String {
    json!({
        "timestamp": "2026-09-12T20:00:00Z",
        "type": "token_usage_record",
        "payload": {
            "response_id": id,
            "model": "gpt-codex",
            "note": pad,
            "usage": {
                "input_tokens": 1,
                "cached_input_tokens": 0,
                "output_tokens": output,
                "reasoning_output_tokens": 0,
                "total_tokens": 1 + output,
            },
        },
    })
    .to_string()
}

fn padded_file(dir: &Path, name: &str, row: impl Fn(&str, u64, &str) -> String) -> PathBuf {
    let path = dir.join(name);
    let pad = "x".repeat(MAX_LINE_BYTES + 1);
    let content = format!("{}\n{}\n", row("first", 7, &pad), row("second", 9, ""));
    std::fs::write(&path, content).unwrap();
    path
}

fn check_fifo(dir: &Path, failures: &mut Vec<String>) {
    let fifo = fifo_in(dir);
    let for_parse = fifo.clone();
    match within_watchdog(move || transcript::parse(&discovered(for_parse), &range())) {
        None => failures.push("transcript::parse on a FIFO did not return in 10s".into()),
        Some(Ok(_)) => failures.push("transcript::parse on a FIFO returned Ok".into()),
        Some(Err(_)) => {}
    }
    match within_watchdog(move || super::codex_provider::normalize(&[fifo], &range())) {
        None => failures.push("codex normalize on a FIFO did not return in 10s".into()),
        Some(result) if result.diagnostics.unreadable_files != 1 => failures.push(format!(
            "codex normalize on a FIFO counted {} unreadable files, expected 1",
            result.diagnostics.unreadable_files
        )),
        Some(_) => {}
    }
}

fn check_claude_overlong(dir: &Path, failures: &mut Vec<String>) {
    let path = padded_file(dir, "claude.jsonl", claude_row);
    let transcript = transcript::parse(&discovered(path), &range()).unwrap();
    let kept: Vec<_> = transcript
        .requests()
        .map(|r| (r.message_id.clone(), r.usage.output))
        .collect();
    if transcript.diagnostics.malformed_rows != 1 {
        failures.push(format!(
            "claude overlong line: malformed_rows = {}, expected 1",
            transcript.diagnostics.malformed_rows
        ));
    }
    if kept != [(Some("second".to_owned()), 9)] {
        failures.push(format!(
            "claude overlong line: kept requests {kept:?}, expected only the second row"
        ));
    }
}

fn check_codex_overlong(dir: &Path, failures: &mut Vec<String>) {
    let path = padded_file(dir, "codex.jsonl", codex_row);
    let result = super::codex_provider::normalize(&[path], &range());
    let kept: Vec<_> = result.rows.iter().map(|r| r.request_id.clone()).collect();
    if result.diagnostics.malformed_rows != 1 {
        failures.push(format!(
            "codex overlong line: malformed_rows = {}, expected 1",
            result.diagnostics.malformed_rows
        ));
    }
    if kept != [Some("second".to_owned())] {
        failures.push(format!(
            "codex overlong line: kept rows {kept:?}, expected only the second row"
        ));
    }
}

#[test]
fn a_fifo_and_an_overlong_line_do_not_stall_or_grow_the_scan() {
    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    check_claude_overlong(dir.path(), &mut failures);
    check_codex_overlong(dir.path(), &mut failures);
    check_fifo(dir.path(), &mut failures);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
