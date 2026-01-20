//! Output formatting utilities for memory commands.

use colored::Colorize;

use crate::commands::common::truncate_for_display;
use crate::fs::memory::{MemoryEntry, MemoryEntryType};

/// Format a single entry for list/query display (compact format)
pub fn format_entry_compact(entry: &MemoryEntry) -> String {
    let time = entry.timestamp.format("%H:%M:%S").to_string();
    let type_emoji = match entry.entry_type {
        MemoryEntryType::Note => "📝",
        MemoryEntryType::Decision => "✅",
        MemoryEntryType::Question => "❓",
    };

    let main_line = format!(
        "{} {} {} {}",
        time.dimmed(),
        type_emoji,
        entry.entry_type.display_name().cyan(),
        truncate_for_display(&entry.content, 50)
    );

    if let Some(ctx) = &entry.context {
        format!(
            "{}\n  {} {}",
            main_line,
            "→".dimmed(),
            truncate_for_display(ctx, 48).yellow()
        )
    } else {
        main_line
    }
}

/// Format a single entry for show display (full format)
pub fn format_entry_full(entry: &MemoryEntry) -> String {
    let time = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
    let type_emoji = match entry.entry_type {
        MemoryEntryType::Note => "📝",
        MemoryEntryType::Decision => "✅",
        MemoryEntryType::Question => "❓",
    };

    let mut output = format!(
        "\n{} {} {}\n{}\n{}",
        type_emoji,
        entry.entry_type.display_name().bold(),
        time.dimmed(),
        "─".repeat(40),
        entry.content
    );

    if let Some(ctx) = &entry.context {
        output.push_str(&format!("\n\n{} {}", "Context:".cyan(), ctx));
    }

    output
}

/// Format session summary for sessions list
pub fn format_session_summary(
    session_id: &str,
    stage_id: Option<&str>,
    total: usize,
    notes: usize,
    decisions: usize,
    questions: usize,
) -> String {
    let stage_info = stage_id
        .map(|s| format!(" [{s}]"))
        .unwrap_or_default();

    format!(
        "{}{} - {} entries (📝 {} / ✅ {} / ❓ {})",
        session_id.cyan(),
        stage_info.dimmed(),
        total,
        notes,
        decisions,
        questions
    )
}

/// Format memory entries for inclusion in a knowledge file
pub fn format_entries_for_knowledge(entries: &[MemoryEntry]) -> String {
    let mut output = String::new();

    // Group by type
    let notes: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
    let decisions: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Decision)
        .collect();
    let questions: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Question)
        .collect();

    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M");
    output.push_str(&format!("## Promoted from Memory [{timestamp}]\n\n"));

    // Format notes as bullet list
    if !notes.is_empty() {
        output.push_str("### Notes\n\n");
        for entry in &notes {
            output.push_str(&format!("- {}\n", entry.content));
        }
        output.push('\n');
    }

    // Format decisions with rationale
    if !decisions.is_empty() {
        output.push_str("### Decisions\n\n");
        for entry in &decisions {
            output.push_str(&format!("- **{}**", entry.content));
            if let Some(ctx) = &entry.context {
                output.push_str(&format!("\n  - *Rationale:* {ctx}"));
            }
            output.push('\n');
        }
        output.push('\n');
    }

    // Format questions as bullet list
    if !questions.is_empty() {
        output.push_str("### Questions\n\n");
        for entry in &questions {
            output.push_str(&format!("- {}\n", entry.content));
        }
        output.push('\n');
    }

    output
}

/// Format a success message for recording an entry
pub fn format_record_success(entry_type: &MemoryEntryType, session_id: &str, text: &str) -> String {
    let (emoji, action) = match entry_type {
        MemoryEntryType::Note => ("📝", "Recorded note"),
        MemoryEntryType::Decision => ("✅", "Recorded decision"),
        MemoryEntryType::Question => ("❓", "Recorded question"),
    };

    format!(
        "{} {} in session '{}'\n  {}",
        emoji.green(),
        action,
        session_id.cyan(),
        truncate_for_display(text, 60)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_entries_for_knowledge() {
        let entries = vec![
            MemoryEntry::new(MemoryEntryType::Note, "Test note".to_string()),
            MemoryEntry::with_context(
                MemoryEntryType::Decision,
                "Use async".to_string(),
                "Better performance".to_string(),
            ),
            MemoryEntry::new(MemoryEntryType::Question, "Why?".to_string()),
        ];

        let formatted = format_entries_for_knowledge(&entries);

        assert!(formatted.contains("## Promoted from Memory"));
        assert!(formatted.contains("### Notes"));
        assert!(formatted.contains("- Test note"));
        assert!(formatted.contains("### Decisions"));
        assert!(formatted.contains("- **Use async**"));
        assert!(formatted.contains("*Rationale:* Better performance"));
        assert!(formatted.contains("### Questions"));
        assert!(formatted.contains("- Why?"));
    }
}
