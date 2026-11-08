use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Utc};
use clap::Parser;
use serde_json::json;
use tempfile::TempDir;

use super::blocks::{render, summarize, Summary};

fn now() -> DateTime<Utc> {
    "2026-11-07T12:00:00Z".parse().unwrap()
}

fn record(ts: &str, stage: &str, role: &str, guard: &str, rule: &str, subject: &str) -> String {
    json!({
        "ts": ts, "stage": stage, "session": "session", "role": role,
        "guard": guard, "rule": rule, "tool": "Bash", "subject": subject,
    })
    .to_string()
}

fn line(ts: &str, stage: &str, subject: &str) -> String {
    record(
        ts,
        stage,
        "main",
        "worktree-isolation",
        "path-traversal",
        subject,
    )
}

fn write_ledger(dir: &Path, stage: &str, lines: &[String]) {
    fs::write(
        dir.join(format!("{stage}.blocks.jsonl")),
        lines.join("\n") + "\n",
    )
    .unwrap();
}

fn summary(lines: &[String]) -> Summary {
    let dir = TempDir::new().unwrap();
    write_ledger(dir.path(), "stage", lines);
    summarize(dir.path(), now(), 24).unwrap()
}

#[test]
fn merges_stages_in_newest_first_order_and_filters_old_records() {
    let dir = TempDir::new().unwrap();
    write_ledger(
        dir.path(),
        "one",
        &[
            line("2026-11-07T10:00:00Z", "one", "first"),
            line("2026-11-05T12:00:00Z", "one", "old"),
        ],
    );
    write_ledger(
        dir.path(),
        "two",
        &[
            line("2026-11-07T11:00:00Z", "two", "newest"),
            line("2026-11-07T09:00:00Z", "two", "last"),
        ],
    );
    fs::write(dir.path().join("ignored.jsonl"), "invalid").unwrap();
    fs::create_dir(dir.path().join("directory.blocks.jsonl")).unwrap();
    let report = summarize(dir.path(), now(), 24).unwrap();
    let subjects: Vec<_> = report.records.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(subjects, ["newest", "first", "last"]);
    assert_eq!(report.records[0].stage, "two");
    assert_eq!(report.records[1].stage, "one");
    assert_eq!(report.malformed, 0);
}

#[test]
fn huge_window_returns_an_error_without_panicking() {
    let dir = TempDir::new().unwrap();
    assert!(summarize(dir.path(), now(), u64::MAX).is_err());
}

#[test]
fn counts_malformed_lines_and_reports_them() {
    let report = summary(&[
        "invalid json".into(),
        line("2026-11-07T11:00:00Z", "s", "ok"),
    ]);
    assert_eq!(report.malformed, 1);
    for by_rule in [false, true] {
        assert!(render(&report, by_rule).ends_with("1 malformed lines skipped"));
    }
    let empty = summary(&["invalid json".into()]);
    assert_eq!(
        render(&empty, false),
        "no hook blocks recorded in the window (1 malformed lines skipped)"
    );
    assert_eq!(summary(&[String::new()]).malformed, 1);
}

#[test]
fn missing_directory_and_empty_file_have_no_records() {
    let dir = TempDir::new().unwrap();
    let missing = summarize(&dir.path().join("missing"), now(), 24).unwrap();
    assert_eq!(
        render(&missing, false),
        "no hook blocks recorded in the window"
    );
    fs::write(dir.path().join("empty.blocks.jsonl"), "").unwrap();
    let empty = summarize(dir.path(), now(), 24).unwrap();
    assert!(empty.records.is_empty());
    assert_eq!(empty.malformed, 0);
}

#[cfg(unix)]
#[test]
fn symlinked_ledger_is_ignored() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target.jsonl");
    fs::write(&target, line("2026-11-07T11:00:00Z", "s", "hidden")).unwrap();
    std::os::unix::fs::symlink(target, dir.path().join("s.blocks.jsonl")).unwrap();
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert!(report.records.is_empty());
    assert_eq!(report.malformed, 0);
}

/// A FIFO at a ledger path is skipped without blocking on open or read.
#[cfg(unix)]
#[test]
fn fifo_ledger_is_skipped_without_blocking() {
    let dir = TempDir::new().unwrap();
    let fifo = dir.path().join("f.blocks.jsonl");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
    write_ledger(
        dir.path(),
        "s",
        &[line("2026-11-07T11:00:00Z", "s", "kept")],
    );
    let report = summarize(dir.path(), now(), 24).unwrap();
    let subjects: Vec<_> = report.records.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(subjects, ["kept"]);
}

#[test]
fn grouped_output_sorts_counts_splits_roles_and_uses_newest_samples() {
    let report = summary(&[
        line("2026-11-07T10:00:00Z", "s", "older"),
        record(
            "2026-11-07T11:00:00Z",
            "s",
            "subagent",
            "worktree-isolation",
            "path-traversal",
            "newer",
        ),
        record(
            "2026-11-07T11:30:00Z",
            "s",
            "main",
            "another",
            "rule",
            "other",
        ),
    ]);
    assert_eq!(
        render(&report, true),
        concat!(
            "2 worktree-isolation/path-traversal main=1 subagent=1\n",
            "  newer\n  older\n1 another/rule main=1 subagent=0\n  other"
        )
    );
}

#[test]
fn grouped_output_breaks_ties_by_name_and_limits_samples() {
    let mut lines = Vec::new();
    for guard in ["z", "a"] {
        for minute in 0..4 {
            lines.push(record(
                &format!("2026-11-07T11:0{minute}:00Z"),
                "s",
                "main",
                guard,
                "rule",
                &format!("sample {minute}"),
            ));
        }
    }
    let output = render(&summary(&lines), true);
    assert!(output.starts_with("4 a/rule main=4 subagent=0\n  sample 3\n  sample 2\n  sample 1"));
    assert!(!output.contains("sample 0"));
}

#[test]
fn multibyte_subjects_truncate_on_character_boundaries() {
    let report = summary(&[line("2026-11-07T11:00:00Z", "s", &"界".repeat(200))]);
    for by_rule in [false, true] {
        let output = render(&report, by_rule);
        assert!(output.ends_with(&"界".repeat(120)));
        assert_eq!(output.matches('界').count(), 120);
    }
}

#[test]
fn decoded_control_characters_cannot_inject_terminal_lines_or_escapes() {
    let report = summary(&[record(
        "2026-11-07T11:00:00Z",
        "s\nstage",
        "main",
        "guard\r",
        "rule\t",
        "hello\n\u{001b}[31mred",
    )]);
    for by_rule in [false, true] {
        let output = render(&report, by_rule);
        assert!(!output.contains('\u{001b}'));
        assert!(!output.contains('\r'));
        assert!(!output.contains('\t'));
        assert!(output.ends_with("hello??[31mred"));
        assert_eq!(output.lines().count(), if by_rule { 2 } else { 1 });
    }
    let mut report = report;
    report.records[0].role = "main\n\u{001b}".into();
    assert!(render(&report, false).contains("main??"));
}

#[test]
fn oversized_lines_are_malformed_even_when_the_json_is_valid() {
    let report = summary(&[
        "x".repeat(5000),
        line("2026-11-07T11:00:00Z", "s", &"x".repeat(5000)),
        line("2026-11-07T11:00:00Z", "s", "kept"),
    ]);
    assert_eq!(report.malformed, 2);
    assert_eq!(report.records.len(), 1);
}

#[test]
fn oversized_file_keeps_newest_records_and_discards_partial_first_line() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("s.blocks.jsonl");
    let mut file = fs::File::create(path).unwrap();
    file.write_all(&vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    writeln!(file).unwrap();
    writeln!(file, "{}", line("2026-11-07T10:00:00Z", "s", "older")).unwrap();
    writeln!(file, "{}", line("2026-11-07T11:00:00Z", "s", "newest")).unwrap();
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[0].subject, "newest");
    assert_eq!(report.malformed, 0);
}

#[test]
fn only_newest_fifty_thousand_lines_are_kept() {
    let dir = TempDir::new().unwrap();
    let mut file = fs::File::create(dir.path().join("s.blocks.jsonl")).unwrap();
    // These old malformed lines must fall outside the retained line window.
    file.write_all(b"old\nold\n").unwrap();
    for _ in 0..49_999 {
        file.write_all(b"bad\n").unwrap();
    }
    writeln!(file, "{}", line("2026-11-07T11:00:00Z", "s", "newest")).unwrap();
    let report = summarize(dir.path(), now(), 24).unwrap();
    assert_eq!(report.records.len(), 1);
    assert_eq!(report.malformed, 49_999);
}

#[test]
fn recent_output_lists_only_twenty_records() {
    let lines = vec![line("2026-11-07T11:00:00Z", "s", "ok"); 21];
    let report = summary(&lines);
    assert_eq!(render(&report, false).lines().count(), 20);
}

#[test]
fn blocks_cli_accepts_report_flags() {
    assert!(crate::cli::Cli::try_parse_from([
        "loom",
        "hook",
        "blocks",
        "--by-rule",
        "--since-hours",
        "24",
    ])
    .is_ok());
}
