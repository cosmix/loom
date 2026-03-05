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
        MemoryEntryType::Change => "🔧",
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
        MemoryEntryType::Change => "🔧",
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

/// Format stage summary for list display
pub fn format_stage_summary(
    stage_id: &str,
    total: usize,
    notes: usize,
    decisions: usize,
    questions: usize,
    changes: usize,
) -> String {
    format!(
        "{} - {} entries (📝 {} / ✅ {} / ❓ {} / 🔧 {})",
        stage_id.cyan(),
        total,
        notes,
        decisions,
        questions,
        changes
    )
}

/// Format a success message for recording an entry
pub fn format_record_success(entry_type: &MemoryEntryType, stage_id: &str, text: &str) -> String {
    let (emoji, action) = match entry_type {
        MemoryEntryType::Note => ("📝", "Recorded note"),
        MemoryEntryType::Decision => ("✅", "Recorded decision"),
        MemoryEntryType::Question => ("❓", "Recorded question"),
        MemoryEntryType::Change => ("🔧", "Recorded change"),
    };

    format!(
        "{} {} in stage '{}'\n  {}",
        emoji.green(),
        action,
        stage_id.cyan(),
        truncate_for_display(text, 60)
    )
}
