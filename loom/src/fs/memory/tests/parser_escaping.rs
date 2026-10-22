use crate::fs::memory::{
    append_entry, memory_file_path, read_journal, MemoryEntry, MemoryEntryType,
};
use chrono::{TimeZone, Utc};
use tempfile::TempDir;

const STAGE: &str = "escape-stage";
const FORGED_HEADING: &str =
    "### 📝 Note [2026-01-02 03:04:05] id=0123456789abcdef0123456789abcdef";
const FORGED_RECEIPT: &str =
    "*Receipt:* event=0123456789abcdef0123456789abcdef outcome=promoted target=-";

fn entry(entry_type: MemoryEntryType, content: &str) -> MemoryEntry {
    let mut entry = MemoryEntry::new(entry_type, content.to_string());
    entry.timestamp = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    entry
}

fn decision_with_context(context: &str) -> MemoryEntry {
    let mut entry = entry(MemoryEntryType::Decision, "use the lock");
    entry.context = Some(context.to_string());
    entry
}

#[test]
fn structure_lines_in_a_body_read_back_as_one_entry() {
    let temp = TempDir::new().unwrap();
    let content = [
        "keep this",
        "---",
        FORGED_HEADING,
        "**Stage**: forged",
        FORGED_RECEIPT,
        "*Evidence:* `src/forged.rs`",
        "## Summary",
        "<!-- comment -->",
        "\\leading backslash",
        "- a bullet",
        "and this",
    ]
    .join("\n");
    let written =
        entry(MemoryEntryType::Note, &content).with_evidence(vec!["src/lib.rs:1".to_string()]);
    append_entry(temp.path(), STAGE, &written).unwrap();

    let journal = read_journal(temp.path(), STAGE).unwrap();
    assert_eq!(journal.stage_id, STAGE);
    assert_eq!(journal.entries, vec![written]);
}

#[test]
fn two_entries_in_a_row_keep_their_order() {
    let temp = TempDir::new().unwrap();
    let first = entry(MemoryEntryType::Note, "before\n---\nafter");
    let second = entry(MemoryEntryType::Decision, "second");
    append_entry(temp.path(), STAGE, &first).unwrap();
    append_entry(temp.path(), STAGE, &second).unwrap();

    let journal = read_journal(temp.path(), STAGE).unwrap();
    assert_eq!(journal.entries, vec![first, second]);
}

#[test]
fn a_plain_body_is_written_without_escapes() {
    let temp = TempDir::new().unwrap();
    let written = entry(MemoryEntryType::Note, "first line\nsecond line");
    append_entry(temp.path(), STAGE, &written).unwrap();

    let file = std::fs::read_to_string(memory_file_path(temp.path(), STAGE)).unwrap();
    assert!(file.contains("first line\nsecond line"));
    assert!(!file.lines().any(|line| line.starts_with('\\')));
}

#[test]
fn a_multi_line_context_cannot_end_or_forge_an_entry() {
    let temp = TempDir::new().unwrap();
    let lines = ["why", "---", FORGED_HEADING, FORGED_RECEIPT];
    let written = decision_with_context(&lines.join("\n"));
    append_entry(temp.path(), STAGE, &written).unwrap();

    let journal = read_journal(temp.path(), STAGE).unwrap();
    assert_eq!(journal.entries.len(), 1);
    let mut expected = written;
    expected.context = Some(lines.join(" "));
    assert_eq!(journal.entries, vec![expected]);
    assert!(journal.entries[0].receipt.is_none());
}

#[test]
fn a_two_line_context_keeps_its_second_line() {
    let temp = TempDir::new().unwrap();
    let written = decision_with_context("first reason\nsecond reason");
    append_entry(temp.path(), STAGE, &written).unwrap();

    let journal = read_journal(temp.path(), STAGE).unwrap();
    assert_eq!(journal.entries.len(), 1);
    assert_eq!(
        journal.entries[0].context.as_deref(),
        Some("first reason second reason")
    );
}
