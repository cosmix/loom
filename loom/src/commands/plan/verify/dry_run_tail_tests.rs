//! Tests for the failed-test note of the dry run and the output tail it carries.

use std::path::Path;

use super::*;
use crate::commands::plan::verify::dry_run::dry_run_tests::{plan, result, stage};
use crate::commands::plan::verify::dry_run::{
    outcome_of, record, stage_commands, DryCommand, DryExecutor,
};
use crate::commands::plan::verify::report::verify_text;

const CARGO_FAILURE: &str = "running 2 tests\ntest a::passes ... ok\ntest a::broken ... FAILED\n\n\
     failures:\n\nfailures:\n    a::broken\n\n\
     test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";

/// The note the dry run records for `command` when it yields `output`.
fn note_for(command: &str, output: (&str, &str)) -> String {
    let text = plan("", &[stage("s", "standard", &[], &[command], "")]);
    let mut report = verify_text(None, Path::new("PLAN.md"), &text);
    let definition = &parse_definition(&text);
    let commands = stage_commands(definition, Default::default(), Path::new("."));
    let (stdout, stderr) = (output.0.to_string(), output.1.to_string());
    let mut execute =
        |c: &DryCommand| Ok((result(&c.command, Some(101), &stdout, &stderr), Vec::new()));
    let execute: &mut DryExecutor<'_> = &mut execute;
    let outcome = outcome_of(execute, &commands[0]);
    record(&mut report, definition, &commands[0], &outcome);
    report.notes.pop().expect("a note")
}

fn parse_definition(text: &str) -> crate::plan::schema::StageDefinition {
    let metadata = crate::plan::parser::parse_plan_metadata(text).unwrap();
    metadata.loom.stages[0].clone()
}

#[test]
fn a_failed_test_run_names_its_failing_tests() {
    let note = note_for(
        "cargo test --lib a::",
        (CARGO_FAILURE, "error: test failed"),
    );
    let (first, tail) = note
        .split_once('\n')
        .expect("the tail follows the first line");
    assert!(first.contains("tests that already exist there fail under this confinement"));
    assert!(first.contains("only if fixing them is the stage's work"));
    assert!(!note.contains("expected before the stage's work"), "{note}");
    assert!(tail.lines().all(|line| line.starts_with("    ")), "{tail}");
    assert!(tail.contains("    a::broken"), "{tail}");
    assert!(tail.contains("test result: FAILED"), "{tail}");
    assert!(
        !tail.contains("error: test failed"),
        "stdout holds the summary: {tail}"
    );
}

#[test]
fn a_failed_command_without_a_summary_keeps_the_old_text() {
    let note = note_for("false", ("", "boom"));
    assert_eq!(
        note,
        "dry run: s acceptance 0 `false` fails at the base commit (expected before the stage's work)"
    );
}

/// `cargo test --all-targets --no-fail-fast`: the first binary fails, the last passes.
const TWO_BINARIES: &str = "running 2 tests\ntest a::passes ... ok\ntest a::broken ... FAILED\n\n\
     failures:\n\nfailures:\n    a::broken\n\n\
     test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n\n\
     error: test failed, to rerun pass `--lib`\n\
     running 3 tests\ntest b::one ... ok\ntest b::two ... ok\ntest b::three ... ok\n\n\
     test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";

#[test]
fn a_failing_first_binary_is_named_though_the_last_one_passes() {
    let excerpt = tail_of(TWO_BINARIES);
    let lines: Vec<&str> = excerpt.lines().collect();
    assert_eq!(lines[0], "test a::broken ... FAILED", "{excerpt}");
    assert!(lines[1].starts_with("test result: FAILED."), "{excerpt}");
    assert!(excerpt.contains("test b::three ... ok"), "{excerpt}");
    assert!(excerpt.ends_with("0 filtered out"), "{excerpt}");
    assert!(!excerpt.contains("a::passes"), "{excerpt}");
}

#[test]
fn without_failure_lines_the_excerpt_is_the_last_six_lines() {
    let text: String = (1..=20).map(|n| format!("line {n}\n\n")).collect();
    let excerpt = tail_of(&text);
    let lines: Vec<&str> = excerpt.lines().collect();
    assert_eq!(lines.len(), 6, "{excerpt}");
    assert_eq!((lines[0], lines[5]), ("line 15", "line 20"));
    assert_eq!(tail_of("a\n\nb\n"), "a\nb");
}

#[test]
fn at_most_twenty_failure_lines_are_kept_and_the_rest_is_counted() {
    let failures: String = (1..=27).map(|n| format!("FAIL t{n}\n")).collect();
    let text = format!("{failures}ok 1\nok 2\nok 3\nok 4\nok 5\nok 6\n");
    let excerpt = tail_of(&text);
    let lines: Vec<&str> = excerpt.lines().collect();
    assert_eq!(lines[0], "FAIL t1", "{excerpt}");
    assert_eq!(lines[19], "FAIL t20", "{excerpt}");
    assert_eq!(lines[20], "… 7 more lines containing FAIL left out");
    assert_eq!(
        &lines[21..],
        ["ok 1", "ok 2", "ok 3", "ok 4", "ok 5", "ok 6"]
    );
}

#[test]
fn a_failure_line_past_the_cap_that_the_tail_prints_is_not_reported_as_left_out() {
    let failures: String = (1..=22).map(|n| format!("FAIL t{n}\n")).collect();
    let excerpt = tail_of(&format!("{failures}ok\n"));
    assert!(!excerpt.contains("left out"), "{excerpt}");
    assert_eq!(excerpt.lines().count(), 23, "{excerpt}");
    assert!(excerpt.ends_with("FAIL t21\nFAIL t22\nok"), "{excerpt}");
}

#[test]
fn no_line_appears_twice() {
    let excerpt = tail_of("ok 1\ntest x ... FAILED\nok 2\ntest result: FAILED\n");
    assert_eq!(
        excerpt,
        "test x ... FAILED\ntest result: FAILED\nok 1\nok 2"
    );
    let lines: Vec<&str> = excerpt.lines().collect();
    let distinct: std::collections::HashSet<&str> = lines.iter().copied().collect();
    assert_eq!(distinct.len(), lines.len(), "{excerpt}");
}

#[test]
fn the_excerpt_keeps_at_most_2000_characters_and_marks_the_cut() {
    let text = format!("FAIL {}\nlast line", "é".repeat(3_000));
    let excerpt = tail_of(&text);
    assert_eq!(excerpt.chars().count(), MAX_CHARS);
    assert!(excerpt.starts_with("FAIL é"));
    assert!(excerpt.ends_with('…'));
    let fits = format!("FAIL {}", "é".repeat(500));
    assert_eq!(tail_of(&fits), fits);
}

#[test]
fn ansi_sequences_and_carriage_returns_do_not_reach_the_tail() {
    let text = "\u{1b}[31mtest a::broken ... FAILED\u{1b}[0m\r\n\u{1b}]0;title\u{7}progress 50%\rdone\u{0}\n";
    let tail = tail_of(text);
    assert_eq!(tail, "test a::broken ... FAILED\nprogress 50%done");
    assert!(!tail.contains(|c: char| c.is_control() && c != '\n'));
}
