//! Integration tests for the stream-json log formatter.
//!
//! Each test reads a .jsonl fixture, passes it through format_stream,
//! and compares the output to the corresponding .expected.txt file.

use loom::commands::container::log_format::{format_stream, FormatOptions};
use std::io::Cursor;
use std::path::Path;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stream-json")
}

fn run_format(fixture_name: &str, opts: FormatOptions) -> String {
    let dir = fixture_dir();
    let jsonl = std::fs::read_to_string(dir.join(format!("{fixture_name}.jsonl")))
        .unwrap_or_else(|e| panic!("Failed to read {fixture_name}.jsonl: {e}"));
    let mut out = Vec::new();
    format_stream(Cursor::new(jsonl.as_bytes()), &opts, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn expected(fixture_name: &str) -> String {
    let dir = fixture_dir();
    std::fs::read_to_string(dir.join(format!("{fixture_name}.expected.txt")))
        .unwrap_or_else(|e| panic!("Failed to read {fixture_name}.expected.txt: {e}"))
}

fn default_opts() -> FormatOptions {
    FormatOptions {
        show_thinking: false,
        verbose: false,
        color: false,
    }
}

#[test]
fn test_assistant_text() {
    let got = run_format("assistant-text", default_opts());
    assert_eq!(
        got,
        expected("assistant-text"),
        "assistant-text format mismatch"
    );
}

#[test]
fn test_tool_use_and_result() {
    let got = run_format("tool-use-and-result", default_opts());
    assert_eq!(
        got,
        expected("tool-use-and-result"),
        "tool-use-and-result format mismatch"
    );
}

#[test]
fn test_tool_use_with_dup_result() {
    let got = run_format("tool-use-with-dup-result", default_opts());
    assert_eq!(
        got,
        expected("tool-use-with-dup-result"),
        "dedup test mismatch"
    );
}

#[test]
fn test_thinking_suppressed_by_default() {
    let got = run_format("thinking-blocks", default_opts());
    assert_eq!(
        got,
        expected("thinking-blocks"),
        "thinking suppression mismatch"
    );
}

#[test]
fn test_thinking_visible_with_flag() {
    let got = run_format(
        "thinking-blocks",
        FormatOptions {
            show_thinking: true,
            verbose: false,
            color: false,
        },
    );
    // With show_thinking=true: expect "[thinking] ..." before the text
    assert!(
        got.contains("[thinking]"),
        "thinking block should appear with show_thinking=true, got: {got:?}"
    );
}

#[test]
fn test_hook_block() {
    let got = run_format("hook-block", default_opts());
    assert_eq!(got, expected("hook-block"), "hook-block format mismatch");
}

#[test]
fn test_hook_warn() {
    let got = run_format("hook-warn", default_opts());
    assert_eq!(got, expected("hook-warn"), "hook-warn format mismatch");
}

#[test]
fn test_rate_limit_verbose_footer() {
    let got = run_format(
        "rate-limit",
        FormatOptions {
            show_thinking: false,
            verbose: true,
            color: false,
        },
    );
    assert_eq!(
        got,
        expected("rate-limit"),
        "verbose footer format mismatch"
    );
}

#[test]
fn test_rate_limit_suppressed_by_default() {
    let got = run_format("rate-limit", default_opts());
    assert!(
        got.is_empty() || got.trim().is_empty(),
        "rate_limit events should be suppressed by default, got: {got:?}"
    );
}

#[test]
fn test_malformed_lines() {
    let got = run_format("malformed", default_opts());
    assert_eq!(
        got,
        expected("malformed"),
        "malformed line handling mismatch"
    );
}

#[test]
fn test_unknown_event() {
    let got = run_format("unknown-event", default_opts());
    assert_eq!(
        got,
        expected("unknown-event"),
        "unknown event format mismatch"
    );
}
