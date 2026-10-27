//! Text the dashboard shows in full: evidence, acceptance criteria and the
//! block detail arrive neutralized but never cut.
use super::tests::{failure_with, summary};
use super::*;
use crate::commands::status::data::FailingCriterion;
use crate::context::untrusted::MAX_INLINE_CHARS;

/// A line well past the inline limit, with an ESC and a bidi override in it.
fn long_hostile_line() -> String {
    format!(
        "{}\u{1b}[2J{}\u{202E}tail",
        "a".repeat(250),
        "b".repeat(200)
    )
}

#[test]
fn a_400_char_evidence_line_arrives_whole_with_its_controls_neutralized() {
    let mut stage = summary();
    let line = long_hostile_line();
    assert!(line.chars().count() > MAX_INLINE_CHARS * 2);
    stage.failure_info = Some(failure_with(vec![line.clone()]));

    sanitize_stage_summary(&mut stage);

    let evidence = stage.failure_info.unwrap().evidence;
    let kept = &evidence[0];
    assert_eq!(kept.chars().count(), line.chars().count());
    assert!(!kept.contains(['\u{1b}', '\u{202E}']));
    assert!(kept.ends_with(" tail"));
}

#[test]
fn every_evidence_line_is_kept() {
    let mut stage = summary();
    let lines: Vec<String> = (0..500).map(|index| format!("line {index}")).collect();
    stage.failure_info = Some(failure_with(lines.clone()));

    sanitize_stage_summary(&mut stage);

    assert_eq!(stage.failure_info.unwrap().evidence, lines);
}

#[test]
fn a_long_acceptance_criterion_is_kept_whole() {
    let mut stage = summary();
    stage.acceptance = vec![long_hostile_line()];

    sanitize_stage_summary(&mut stage);

    let criterion = &stage.acceptance[0];
    assert_eq!(
        criterion.chars().count(),
        long_hostile_line().chars().count()
    );
    assert!(!criterion.contains(['\u{1b}', '\u{202E}']));
}

#[test]
fn failing_criteria_are_neutralized_without_a_cut() {
    let mut stage = summary();
    stage.block_detail.failing_criteria = vec![FailingCriterion {
        index: 4,
        command: long_hostile_line(),
        diagnostic: Some(long_hostile_line()),
    }];

    sanitize_stage_summary(&mut stage);

    let criterion = &stage.block_detail.failing_criteria[0];
    let whole = long_hostile_line().chars().count();
    assert_eq!(criterion.command.chars().count(), whole);
    assert_eq!(
        criterion.diagnostic.as_deref().map(|d| d.chars().count()),
        Some(whole)
    );
    assert!(!criterion.command.contains('\u{1b}'));
}

#[test]
fn a_combining_mark_flood_in_evidence_keeps_four_marks_and_every_other_character() {
    let mut stage = summary();
    let flood = format!("x{} {}", "\u{0301}".repeat(100_000), "y".repeat(300));
    stage.failure_info = Some(failure_with(vec![flood]));

    sanitize_stage_summary(&mut stage);

    let evidence = stage.failure_info.unwrap().evidence;
    let expected = format!("x{} {}", "\u{0301}".repeat(4), "y".repeat(300));
    assert_eq!(evidence, [expected]);
}
