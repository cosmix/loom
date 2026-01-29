//! Per-session memory journal for continuous fact recording.
//!
//! Memory journals allow agents to continuously record notes, decisions, and questions
//! during a session. This implements the Manus todo.md recitation pattern:
//! - Agent constantly writes to memory
//! - Signal generation recites recent memory at end
//! - Keeps important context in attention window
//!
//! Memory files are stored in .work/memory/{session-id}.md
//!
//! Entry types:
//! - Note: General observations and context
//! - Decision: Choices made with rationale
//! - Question: Open questions for future investigation

mod constants;
mod export;
mod parser;
mod persistence;
mod query;
mod storage;
mod types;

// Re-export public types
pub use types::{MemoryEntry, MemoryEntryType, MemoryJournal};

// Re-export storage functions
pub use storage::{
    append_entry, create_journal, init_memory_dir, memory_dir, memory_file_path, read_journal,
    write_summary,
};

// Re-export query functions
pub use query::{generate_summary, get_recent_entries, query_entries};

// Re-export export functions
pub use export::{format_memory_for_handoff, format_memory_for_signal};

// Re-export persistence functions
pub use persistence::{
    delete_entries_by_type, extract_key_notes, list_journals, preserve_for_crash, validate_content,
};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_entry_type_display() {
        assert_eq!(MemoryEntryType::Note.to_string(), "note");
        assert_eq!(MemoryEntryType::Decision.to_string(), "decision");
        assert_eq!(MemoryEntryType::Question.to_string(), "question");
    }

    #[test]
    fn test_entry_type_from_str() {
        assert_eq!(
            "note".parse::<MemoryEntryType>().unwrap(),
            MemoryEntryType::Note
        );
        assert_eq!(
            "DECISION".parse::<MemoryEntryType>().unwrap(),
            MemoryEntryType::Decision
        );
        assert_eq!(
            "questions".parse::<MemoryEntryType>().unwrap(),
            MemoryEntryType::Question
        );
        assert!("invalid".parse::<MemoryEntryType>().is_err());
    }

    #[test]
    fn test_create_and_read_journal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "test-session-123";
        create_journal(work_dir, session_id, Some("test-stage")).unwrap();

        let journal = read_journal(work_dir, session_id).unwrap();
        assert_eq!(journal.session_id, session_id);
        assert_eq!(journal.stage_id.as_deref(), Some("test-stage"));
        assert!(journal.entries.is_empty());
    }

    #[test]
    fn test_append_and_read_entries() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "test-session-456";
        create_journal(work_dir, session_id, None).unwrap();

        let entry1 = MemoryEntry::new(MemoryEntryType::Note, "Found important pattern".to_string());
        append_entry(work_dir, session_id, &entry1).unwrap();

        let entry2 = MemoryEntry::with_context(
            MemoryEntryType::Decision,
            "Use builder pattern for config".to_string(),
            "Provides better API ergonomics".to_string(),
        );
        append_entry(work_dir, session_id, &entry2).unwrap();

        let entry3 = MemoryEntry::new(
            MemoryEntryType::Question,
            "Should we cache results?".to_string(),
        );
        append_entry(work_dir, session_id, &entry3).unwrap();

        let journal = read_journal(work_dir, session_id).unwrap();
        assert_eq!(journal.entries.len(), 3);

        assert_eq!(journal.entries[0].entry_type, MemoryEntryType::Note);
        assert!(journal.entries[0].content.contains("important pattern"));

        assert_eq!(journal.entries[1].entry_type, MemoryEntryType::Decision);
        assert!(journal.entries[1].context.is_some());

        assert_eq!(journal.entries[2].entry_type, MemoryEntryType::Question);
    }

    #[test]
    fn test_format_memory_for_signal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "signal-test";
        create_journal(work_dir, session_id, None).unwrap();

        let entry1 = MemoryEntry::new(MemoryEntryType::Note, "Note 1".to_string());
        let entry2 = MemoryEntry::new(MemoryEntryType::Decision, "Decision 1".to_string());
        let entry3 = MemoryEntry::new(MemoryEntryType::Question, "Question 1".to_string());

        append_entry(work_dir, session_id, &entry1).unwrap();
        append_entry(work_dir, session_id, &entry2).unwrap();
        append_entry(work_dir, session_id, &entry3).unwrap();

        let signal = format_memory_for_signal(work_dir, session_id, 10).unwrap();
        assert!(signal.contains("### Notes"));
        assert!(signal.contains("Note 1"));
        assert!(signal.contains("### Decisions"));
        assert!(signal.contains("Decision 1"));
        assert!(signal.contains("### Open Questions"));
        assert!(signal.contains("Question 1"));
    }

    #[test]
    fn test_query_entries() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "query-test";
        create_journal(work_dir, session_id, None).unwrap();

        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Authentication flow".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Database schema".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Decision, "Use JWT for auth".to_string()),
        )
        .unwrap();

        let journal = read_journal(work_dir, session_id).unwrap();

        let results = query_entries(&journal, "auth");
        assert_eq!(results.len(), 2);

        let results = query_entries(&journal, "database");
        assert_eq!(results.len(), 1);

        let results = query_entries(&journal, "nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_generate_summary() {
        let journal = MemoryJournal {
            session_id: "summary-test".to_string(),
            stage_id: Some("test-stage".to_string()),
            entries: vec![
                MemoryEntry::new(MemoryEntryType::Note, "Note 1".to_string()),
                MemoryEntry::new(MemoryEntryType::Note, "Note 2".to_string()),
                MemoryEntry::new(MemoryEntryType::Decision, "Decision 1".to_string()),
                MemoryEntry::new(MemoryEntryType::Question, "Question 1".to_string()),
            ],
            summary: None,
        };

        let summary = generate_summary(&journal, 5);
        assert!(summary.contains("## Summary"));
        assert!(summary.contains("**Total entries**: 4"));
        assert!(summary.contains("**Notes**: 2"));
        assert!(summary.contains("**Decisions**: 1"));
        assert!(summary.contains("**Questions**: 1"));
        assert!(summary.contains("### Key Decisions"));
        assert!(summary.contains("### Open Questions"));
    }

    #[test]
    fn test_format_memory_for_handoff() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "handoff-test";
        create_journal(work_dir, session_id, None).unwrap();

        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Important note".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::with_context(
                MemoryEntryType::Decision,
                "Key decision".to_string(),
                "Good rationale".to_string(),
            ),
        )
        .unwrap();

        let handoff = format_memory_for_handoff(work_dir, session_id).unwrap();
        assert!(handoff.contains("## Session Memory"));
        assert!(handoff.contains("### Decisions Made"));
        assert!(handoff.contains("Key decision"));
        assert!(handoff.contains("Good rationale"));
        assert!(handoff.contains("### Recent Notes"));
        assert!(handoff.contains("Important note"));
    }

    #[test]
    fn test_preserve_for_crash() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "crash-test";
        create_journal(work_dir, session_id, None).unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Important work".to_string()),
        )
        .unwrap();

        let preserved = preserve_for_crash(work_dir, session_id).unwrap().unwrap();
        assert!(preserved.exists());
        assert!(preserved.to_string_lossy().contains("memory-crash-test.md"));

        let content = std::fs::read_to_string(&preserved).unwrap();
        assert!(content.contains("Important work"));
    }

    #[test]
    fn test_extract_key_notes() {
        let journal = MemoryJournal {
            session_id: "extract-test".to_string(),
            stage_id: None,
            entries: vec![
                MemoryEntry::new(MemoryEntryType::Note, "Just a note".to_string()),
                MemoryEntry::with_context(
                    MemoryEntryType::Decision,
                    "Use pattern X".to_string(),
                    "Because Y".to_string(),
                ),
                MemoryEntry::new(MemoryEntryType::Decision, "Another decision".to_string()),
            ],
            summary: None,
        };

        let key_notes = extract_key_notes(&journal);
        assert_eq!(key_notes.len(), 2);
        assert!(key_notes[0].contains("Use pattern X"));
        assert!(key_notes[0].contains("Because Y"));
        assert!(key_notes[1].contains("Another decision"));
    }

    #[test]
    fn test_validate_content() {
        assert!(validate_content("Valid content").is_ok());
        assert!(validate_content("").is_err());
        assert!(validate_content(&"a".repeat(2001)).is_err());
    }

    #[test]
    fn test_list_journals() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        create_journal(work_dir, "session-1", None).unwrap();
        create_journal(work_dir, "session-2", None).unwrap();

        let journals = list_journals(work_dir).unwrap();
        assert_eq!(journals.len(), 2);
        assert!(journals.contains(&"session-1".to_string()));
        assert!(journals.contains(&"session-2".to_string()));
    }

    #[test]
    fn test_delete_entries_by_type_single() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "delete-single-test";
        create_journal(work_dir, session_id, None).unwrap();

        // Add entries of different types
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Note 1".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Decision, "Decision 1".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Question, "Question 1".to_string()),
        )
        .unwrap();

        // Delete only notes
        let deleted =
            delete_entries_by_type(work_dir, session_id, Some(MemoryEntryType::Note)).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].entry_type, MemoryEntryType::Note);
        assert!(deleted[0].content.contains("Note 1"));

        // Verify remaining entries
        let journal = read_journal(work_dir, session_id).unwrap();
        assert_eq!(journal.entries.len(), 2);
        assert!(journal
            .entries
            .iter()
            .all(|e| e.entry_type != MemoryEntryType::Note));
    }

    #[test]
    fn test_delete_entries_by_type_all() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "delete-all-test";
        create_journal(work_dir, session_id, None).unwrap();

        // Add entries of different types
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Note, "Note 1".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Decision, "Decision 1".to_string()),
        )
        .unwrap();
        append_entry(
            work_dir,
            session_id,
            &MemoryEntry::new(MemoryEntryType::Question, "Question 1".to_string()),
        )
        .unwrap();

        // Delete all (entry_type = None)
        let deleted = delete_entries_by_type(work_dir, session_id, None).unwrap();
        assert_eq!(deleted.len(), 3);

        // Verify journal is now empty
        let journal = read_journal(work_dir, session_id).unwrap();
        assert!(journal.entries.is_empty());
    }

    #[test]
    fn test_delete_entries_by_type_empty_journal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let session_id = "delete-empty-test";
        create_journal(work_dir, session_id, None).unwrap();

        // Delete from empty journal
        let deleted =
            delete_entries_by_type(work_dir, session_id, Some(MemoryEntryType::Note)).unwrap();
        assert!(deleted.is_empty());
    }

    #[test]
    fn test_delete_entries_by_type_nonexistent_session() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Delete from nonexistent session
        let deleted = delete_entries_by_type(work_dir, "nonexistent", None).unwrap();
        assert!(deleted.is_empty());
    }
}
