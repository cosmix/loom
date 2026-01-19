//! Export and formatting functions for memory journal content.

use super::query::get_recent_entries;
use super::storage::read_journal;
use super::types::MemoryEntryType;
use std::path::Path;

/// Truncate content for display
pub fn truncate_content(s: &str, max_len: usize) -> String {
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    if single_line.len() <= max_len {
        single_line
    } else {
        format!("{}…", &single_line[..max_len - 1])
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
