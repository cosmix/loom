//! The needs-attention panel: how each entry is laid out in the ledger.

use ratatui::text::{Line, Span};

use super::panels::{attention_line_count, attention_lines, fit_to_rows};
use super::tests::make_stage;
use crate::commands::status::render::attention_model::{
    attention_entries, Alternative, AttentionEntry,
};
use crate::models::stage::StageStatus;

fn entry(label: &'static str, command: Option<&str>, note: Option<&str>) -> AttentionEntry {
    AttentionEntry {
        id: "stage-a".into(),
        name: "Stage A".into(),
        label,
        command: command.map(str::to_owned),
        command_when: None,
        alternatives: Vec::new(),
        acceptance: Vec::new(),
        note: note.map(str::to_owned),
        automatic: false,
        failure_type: None,
        evidence: Vec::new(),
        review_reason: None,
        cleanup_warning: None,
        has_human_review_choices: false,
        dispute_count: None,
        judge_heartbeat_secs: None,
        completion_blocker: None,
        outgoing_session_exit_reason: None,
    }
}

fn rendered_at(entry: AttentionEntry, width: u16) -> Vec<String> {
    attention_lines(&[entry], width)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn rendered(entry: AttentionEntry) -> Vec<String> {
    rendered_at(entry, 120)
}

#[test]
fn human_review_attention_lists_the_three_full_commands() {
    let lines = rendered(AttentionEntry {
        review_reason: Some("ambiguous result".into()),
        has_human_review_choices: true,
        ..entry("NEEDS REVIEW", None, None)
    });
    assert_eq!(lines.len(), 5, "{lines:#?}");
    assert!(lines[2].ends_with("→ loom stage human-review stage-a --approve"));
    assert!(lines[3].ends_with("→ loom stage human-review stage-a --force-complete"));
    assert!(lines[4].ends_with("→ loom stage human-review stage-a --reject \"<reason>\""));
}

#[test]
fn entry_without_detail_omits_the_dangling_separator() {
    let lines = rendered(entry("NEEDS INPUT", None, Some("answer it")));
    assert!(lines[1].ends_with("NEEDS INPUT"));
    assert!(!lines[1].contains(" · "));
}

#[test]
fn a_header_detail_cut_at_the_width_ends_in_an_ellipsis() {
    let reason = "a review reason far longer than the room the header line leaves";
    let lines = rendered_at(
        AttentionEntry {
            review_reason: Some(reason.into()),
            ..entry("NEEDS REVIEW", None, None)
        },
        64,
    );
    assert!(lines[1].ends_with('…'), "{lines:#?}");
}

#[test]
fn a_note_alone_has_no_command_arrow_and_a_note_comes_before_its_command() {
    let note_only = rendered(entry("NEEDS INPUT", None, Some("answer it")));
    assert_eq!(note_only[2].trim(), "answer it");
    let both = rendered(entry(
        "BLOCKED",
        Some("loom stage retry stage-a --force"),
        Some("retry limit reached (3/3)"),
    ));
    assert_eq!(both.len(), 4, "{both:#?}");
    assert_eq!(both[2].trim(), "retry limit reached (3/3)");
    assert!(
        both[3].ends_with("→ loom stage retry stage-a --force"),
        "{both:#?}"
    );
}

#[test]
fn a_long_note_wraps_under_the_entry_instead_of_running_off_the_line() {
    let reason = "criterion 5 cannot pass in this sandbox because it denies AF_UNIX socket \
                  creation everywhere, so the stage can never finish";
    let note = format!("blocked by the stage agent: {reason}");
    let lines = rendered_at(
        entry("BLOCKED", Some("loom stage retry stage-a"), Some(&note)),
        80,
    );

    let (command, rows) = lines.split_last().unwrap();
    let note_rows: Vec<&str> = rows[2..].iter().map(|line| line.trim()).collect();
    assert!(note_rows.len() >= 2, "{lines:#?}");
    assert_eq!(note_rows.join(" "), note);
    assert!(
        lines.iter().all(|line| line.chars().count() <= 80),
        "{lines:#?}"
    );
    assert!(command.ends_with("→ loom stage retry stage-a"));
}

#[test]
fn a_note_past_the_line_limit_ends_in_an_ellipsis_and_keeps_the_command() {
    let note = "word ".repeat(200);
    let lines = rendered_at(
        entry("BLOCKED", Some("loom stage retry stage-a"), Some(&note)),
        80,
    );

    assert_eq!(lines.len(), 6, "{lines:#?}");
    assert!(lines[4].ends_with('…'), "{lines:#?}");
    assert!(lines[5].ends_with("→ loom stage retry stage-a"));
}

#[test]
fn the_blank_lines_of_a_note_take_no_rows() {
    let note = "blocked by the agent: run it\n\nthen report";
    let lines = rendered(entry("BLOCKED", None, Some(note)));

    assert_eq!(lines.len(), 4, "{lines:#?}");
    assert_eq!(lines[2].trim(), "blocked by the agent: run it");
    assert_eq!(lines[3].trim(), "then report");
}

#[test]
fn when_to_run_the_command_comes_before_it_and_alternatives_follow() {
    let alternative = Alternative {
        when: "you give up".into(),
        command: "loom stage skip stage-a".into(),
    };
    let lines = rendered(AttentionEntry {
        command_when: Some("you have done what the reason asks".into()),
        alternatives: vec![alternative],
        ..entry(
            "BLOCKED",
            Some("loom stage retry stage-a"),
            Some("run it first"),
        )
    });

    assert_eq!(lines.len(), 7, "{lines:#?}");
    assert_eq!(lines[2].trim(), "run it first");
    assert_eq!(lines[3].trim(), "Once you have done what the reason asks:");
    assert!(lines[4].ends_with("→ loom stage retry stage-a"));
    assert_eq!(lines[5].trim(), "If you give up:");
    assert!(lines[6].ends_with("→ loom stage skip stage-a"));
}

#[test]
fn line_count_is_the_rendered_entry_lines() {
    let review = AttentionEntry {
        has_human_review_choices: true,
        ..entry("NEEDS REVIEW", None, None)
    };
    let blocked = entry("BLOCKED", Some("loom stage retry stage-a"), None);

    assert_eq!(attention_line_count(&[review, blocked], 120), 6);
}

#[test]
fn an_entry_with_neither_command_nor_note_prints_no_empty_line() {
    let lines = rendered(entry("NEEDS REVIEW", None, None));
    assert_eq!(lines.len(), 2, "{lines:#?}");
}

/// A stage its agent blocked over a criterion, as the panel receives it: the
/// retry, then amend-delete, amend-replace and skip, and the criteria they number.
fn blocked_by_agent(criteria: &[&str]) -> AttentionEntry {
    named_blocked_by_agent("stage-a", criteria)
}

fn named_blocked_by_agent(id: &str, criteria: &[&str]) -> AttentionEntry {
    let mut stage = make_stage(id, StageStatus::Blocked);
    stage.close_reason = Some("criterion 1 cannot pass in this sandbox".into());
    stage.acceptance = criteria.iter().map(|&criterion| criterion.into()).collect();
    attention_entries(&[stage]).remove(0)
}

fn width_of(line: &str) -> usize {
    line.chars().count()
}

#[test]
fn each_alternative_keeps_its_own_when_and_command_at_80_and_120_columns() {
    for width in [80_u16, 120] {
        let entry = blocked_by_agent(&["cargo test", "bun run check"]);
        let lines = rendered_at(entry, width);

        assert!(
            lines
                .iter()
                .all(|line| width_of(line) <= usize::from(width)),
            "{width}: {lines:#?}"
        );
        let whens: Vec<&str> = lines
            .iter()
            .map(|line| line.trim())
            .filter(|line| line.starts_with("If "))
            .collect();
        assert_eq!(whens.len(), 3, "{width}: {lines:#?}");
        assert!(
            whens[0].starts_with("If the reason is an acceptance criterion"),
            "{width}: {whens:#?}"
        );
        assert!(
            whens[1].starts_with("If that criterion should run another way"),
            "{width}: {whens:#?}"
        );
        assert!(
            whens[2].starts_with("If you give up on the stage"),
            "{width}: {whens:#?}"
        );
        for (index, line) in lines.iter().enumerate() {
            if line.trim().starts_with("If ") {
                assert!(
                    lines[index + 1].contains("→ loom stage "),
                    "{width}: {lines:#?}"
                );
            }
        }
    }
}

#[test]
fn a_command_or_situation_cut_to_the_width_ends_in_an_ellipsis() {
    let lines = rendered_at(blocked_by_agent(&["cargo test"]), 80);

    let cut: Vec<&String> = lines.iter().filter(|line| line.ends_with('…')).collect();
    assert!(
        cut.iter().any(|line| line.contains("If the reason is")),
        "{lines:#?}"
    );
    assert!(
        cut.iter()
            .any(|line| line.contains("--op replace --index <n> --valu")),
        "{lines:#?}"
    );
    assert!(cut.iter().all(|line| width_of(line) == 80), "{cut:#?}");
    for whole in [
        "→ loom stage amend stage-a --field acceptance --op delete --index <n>",
        "→ loom stage skip stage-a",
    ] {
        let line = lines.iter().find(|line| line.ends_with(whole));
        assert!(line.is_some(), "{whole}: {lines:#?}");
    }
}

#[test]
fn at_80_columns_the_delete_and_replace_commands_of_a_long_stage_id_differ_on_screen() {
    let lines = rendered_at(
        named_blocked_by_agent("integration-verify", &["cargo test"]),
        80,
    );

    let amend: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("→ loom stage amend integration-verify"))
        .collect();
    assert_eq!(amend.len(), 2, "{lines:#?}");
    assert!(amend[0].contains("--op delete"), "{amend:#?}");
    assert!(amend[1].contains("--op replace"), "{amend:#?}");
    assert!(amend.iter().all(|line| width_of(line) <= 80), "{amend:#?}");
}

#[test]
fn an_entry_with_criteria_points_to_loom_status_for_their_numbers() {
    let with = rendered_at(blocked_by_agent(&["cargo test", "bun run check"]), 80);
    let pointer = with.last().unwrap();
    assert_eq!(
        pointer.trim(),
        "Acceptance criteria are numbered from 0 in `loom status`"
    );
    assert!(width_of(pointer) <= 80);

    let without = rendered_at(blocked_by_agent(&[]), 80);
    assert!(
        !without
            .iter()
            .any(|line| line.contains("Acceptance criteria")),
        "{without:#?}"
    );
}

#[test]
fn line_count_covers_every_line_of_a_blocked_entry() {
    for width in [80_u16, 120] {
        let entry = blocked_by_agent(&["cargo test"]);
        let count = attention_line_count(std::slice::from_ref(&entry), width);

        assert_eq!(count + 1, attention_lines(&[entry], width).len());
        assert!(count <= 13, "{width}: {count}");
    }
}

#[test]
fn a_panel_given_too_few_rows_ends_in_a_row_counting_what_it_left_out() {
    let lines: Vec<Line<'static>> = (0..8)
        .map(|n| Line::from(Span::raw(format!("line {n}"))))
        .collect();

    let fitted: Vec<String> = fit_to_rows(lines.clone(), 5, 80)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(fitted.len(), 5);
    assert_eq!(fitted[3], "line 3");
    assert_eq!(fitted[4], " … 4 more lines, `loom status` shows them all");

    assert_eq!(fit_to_rows(lines.clone(), 8, 80).len(), 8);
    assert_eq!(fit_to_rows(lines, 9, 80).len(), 8);
}
