//! Export and formatting functions for memory journal content.

use super::query::get_recent_entries;
use super::storage::read_journal;
use super::types::MemoryEntryType;
use std::path::Path;

/// Truncate content for display (UTF-8 safe)
pub fn truncate_content(s: &str, max_len: usize) -> String {
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    let char_count = single_line.chars().count();
    if char_count <= max_len {
        single_line
    } else {
        format!(
            "{}…",
            single_line.chars().take(max_len - 1).collect::<String>()
        )
    }
}

/// Format memory entries for embedding in a signal
pub fn format_memory_for_signal(
    work_dir: &Path,
    session_id: &str,
    max_entries: usize,
) -> Option<String> {
    let journal = read_journal(work_dir, session_id).ok()?;

    if journal.entries.is_empty() {
        return None;
    }

    let recent = get_recent_entries(&journal, max_entries);
    if recent.is_empty() {
        return None;
    }

    let mut output = String::new();

    // Group by type for better organization
    let notes: Vec<_> = recent
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    let decisions: Vec<_> = recent
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    let questions: Vec<_> = recent
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Question)
        .collect();

    if !notes.is_empty() {
        output.push_str("### Notes\n\n");
        for entry in notes {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                truncate_content(&entry.content, 150)
            ));
        }
        output.push('\n');
    }

    if !decisions.is_empty() {
        output.push_str("### Decisions\n\n");
        for entry in decisions {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                truncate_content(&entry.content, 150)
            ));
            if let Some(ctx) = &entry.context {
                output.push_str(&format!(
                    "  - *Rationale:* {}\n",
                    truncate_content(ctx, 100)
                ));
            }
        }
        output.push('\n');
    }

    if !questions.is_empty() {
        output.push_str("### Open Questions\n\n");
        for entry in questions {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                truncate_content(&entry.content, 150)
            ));
        }
        output.push('\n');
    }

    Some(output)
}

/// Format memory for inclusion in handoff file
pub fn format_memory_for_handoff(work_dir: &Path, session_id: &str) -> Option<String> {
    let journal = read_journal(work_dir, session_id).ok()?;

    if journal.entries.is_empty() {
        return None;
    }

    let mut output = String::new();
    output.push_str("## Session Memory\n\n");
    output.push_str(&format!(
        "Memory journal from session {} ({} entries).\n\n",
        session_id,
        journal.entries.len()
    ));

    // Include all decisions and questions (they're important for handoffs)
    let decisions: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    let questions: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Question)
        .collect();

    if !decisions.is_empty() {
        output.push_str("### Decisions Made\n\n");
        for entry in &decisions {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                entry.content
            ));
            if let Some(ctx) = &entry.context {
                output.push_str(&format!("  - *Rationale:* {ctx}\n"));
            }
        }
        output.push('\n');
    }

    if !questions.is_empty() {
        output.push_str("### Open Questions\n\n");
        for entry in &questions {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                entry.content
            ));
        }
        output.push('\n');
    }

    // Recent notes (last 5)
    let notes: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    if !notes.is_empty() {
        output.push_str("### Recent Notes\n\n");
        for entry in notes.iter().rev().take(5) {
            output.push_str(&format!(
                "- **[{}]** {}\n",
                entry.timestamp.format("%H:%M"),
                truncate_content(&entry.content, 200)
            ));
        }
        output.push('\n');
    }

    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_content_short() {
        let result = truncate_content("hello world", 20);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_truncate_content_exact() {
        let result = truncate_content("hello", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_content_long() {
        let result = truncate_content("hello world", 8);
        assert_eq!(result, "hello w…");
    }

    #[test]
    fn test_truncate_content_multiline() {
        let result = truncate_content("hello\nworld", 20);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_truncate_content_utf8_emoji() {
        // Emoji are multi-byte (4 bytes each)
        // "🎉🎊🎁" = 3 chars but 12 bytes
        let input = "🎉🎊🎁🎈🎂";
        let result = truncate_content(input, 4);
        // Should truncate to 3 chars + ellipsis
        assert_eq!(result, "🎉🎊🎁…");
        // Verify no panic on multi-byte boundary
    }

    #[test]
    fn test_truncate_content_utf8_cjk() {
        // CJK characters are 3 bytes each
        let input = "你好世界";
        let result = truncate_content(input, 3);
        assert_eq!(result, "你好…");
    }

    #[test]
    fn test_truncate_content_utf8_mixed() {
        // Mix ASCII and multi-byte
        // "hello🎉world" = 11 chars (5 ASCII + 1 emoji + 5 ASCII)
        let input = "hello🎉world";
        let result = truncate_content(input, 8);
        // max=8: take 7 chars + ellipsis = "hello🎉w…"
        assert_eq!(result, "hello🎉w…");
    }
}
