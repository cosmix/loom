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
        |c: &DryCommand| (result(&c.command, Some(101), &stdout, &stderr), Vec::new());
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

#[test]
fn the_tail_keeps_twelve_lines_and_marks_the_cut() {
    let text: String = (1..=20).map(|n| format!("line {n}\n\n")).collect();
    let tail = tail_of(&text);
    let lines: Vec<&str> = tail.lines().collect();
    assert_eq!(lines.len(), 12, "{tail}");
    assert_eq!(lines[0], "…line 9");
    assert_eq!(lines[11], "line 20");
    assert_eq!(tail_of("a\n\nb\n"), "a\nb");
}

#[test]
fn the_tail_keeps_at_most_1200_characters_from_the_end() {
    let text = format!("{}\nlast line", "é".repeat(2_000));
    let tail = tail_of(&text);
    assert_eq!(tail.chars().count(), MAX_CHARS);
    assert!(tail.starts_with("…é"));
    assert!(tail.ends_with("\nlast line"));
}

#[test]
fn ansi_sequences_and_carriage_returns_do_not_reach_the_tail() {
    let text = "\u{1b}[31mtest a::broken ... FAILED\u{1b}[0m\r\n\u{1b}]0;title\u{7}progress 50%\rdone\u{0}\n";
    let tail = tail_of(text);
    assert_eq!(tail, "test a::broken ... FAILED\nprogress 50%done");
    assert!(!tail.contains(|c: char| c.is_control() && c != '\n'));
}
