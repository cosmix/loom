//! Column-alignment checks for the ledger table, split out to keep
//! `tests.rs` under the file line limit.

use super::tests::{fixture, render_view};

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
