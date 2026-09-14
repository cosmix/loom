//! Column-alignment checks for the ledger table, split out to keep
//! `tests.rs` under the file line limit.

use super::{
    cells::activity_cell,
    tests::{contains, fixture, make_blocker, make_stage, render_view},
    text::{padded, text_width},
};
use crate::{commands::status::data::CompletionBlockerState, models::stage::StageStatus};

/// Byte offset of the last terminal-buffer row's cell that renders `needle`,
/// as a character index - `str::find`/`rfind` return byte offsets, which
/// desync from column position once a multi-byte character (e.g. a wide
/// icon) appears earlier in the row.
fn column_of(row: &str, needle: &str) -> Option<usize> {
    row.find(needle)
        .map(|byte_index| row[..byte_index].chars().count())
}

fn last_column_of(row: &str, needle: &str) -> Option<usize> {
    row.rfind(needle)
        .map(|byte_index| row[..byte_index].chars().count())
}

#[test]
fn wide_icon_row_keeps_column_alignment() {
    let data = fixture();
    let rows = render_view(&data, 120, 40, false);
    let header = rows.iter().find(|row| row.contains("STAGE")).unwrap();
    let stage_at = column_of(header, "STAGE").unwrap();
    let models_at = column_of(header, "MODELS").unwrap();
    let merge_at = column_of(header, "MERGE").unwrap();

    // s-conflict's state icon (⚡) is two cells wide; s-completed's (✓) is one.
    let wide_row = rows.iter().find(|row| row.contains("s-conflict")).unwrap();
    let narrow_row = rows.iter().find(|row| row.contains("s-completed")).unwrap();

    assert_eq!(column_of(wide_row, "s-conflict"), Some(stage_at));
    assert_eq!(column_of(narrow_row, "s-completed"), Some(stage_at));

    // Both rows show a MODELS cell starting with "opus"; the Activity column
    // for MergeConflict also reads "conflict", so use MODELS/MERGE-specific
    // needles rather than reusing "conflict" for both checks.
    assert_eq!(column_of(wide_row, "opus"), Some(models_at));
    assert_eq!(column_of(narrow_row, "opus"), Some(models_at));

    // MERGE is the rightmost column, so the last "conflict" in the wide row
    // is its MERGE cell, not the earlier one in ACTIVITY.
    assert_eq!(last_column_of(wide_row, "conflict"), Some(merge_at));
    assert_eq!(column_of(narrow_row, "unmerged"), Some(merge_at));
}

#[test]
fn wide_terminal_widens_stage_column() {
    let data = fixture();
    let at_120 = render_view(&data, 120, 40, false);
    let at_140 = render_view(&data, 140, 40, false);
    let header_120 = at_120
        .iter()
        .find(|row| row.contains("DEPENDS ON"))
        .unwrap();
    let header_140 = at_140
        .iter()
        .find(|row| row.contains("DEPENDS ON"))
        .unwrap();
    assert!(header_140.find("DEPENDS ON").unwrap() > header_120.find("DEPENDS ON").unwrap());
}

#[test]
fn drops_columns_in_priority_order() {
    let data = fixture();
    let at_110 = render_view(&data, 110, 40, false);
    assert!(!contains(&at_110, "TIME"));
    assert!(!contains(&at_110, "MODELS"));
    assert!(contains(&at_110, "DEPENDS ON"));

    let at_74 = render_view(&data, 74, 40, false);
    assert!(!contains(&at_74, "CONTEXT"));
    assert!(contains(&at_74, "MERGE"));
}

#[test]
fn wide_blocker_summary_truncates_to_activity_width() {
    let mut stage = make_stage("wide", StageStatus::Executing);
    stage.completion_blocker = Some(make_blocker(
        CompletionBlockerState::Pending,
        "界界界🚧界界界🚧 this remains long",
    ));
    let cell = activity_cell(&stage, 24);
    assert!(cell.text.ends_with('…'));
    assert!(cell.text.contains('界'));
    assert_eq!(text_width(&padded(&cell.text, 24)), 24);
}
