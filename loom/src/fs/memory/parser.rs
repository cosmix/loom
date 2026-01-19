//! Parsing and formatting functions for memory journal entries.

use super::types::{EntryBuilder, MemoryEntry, MemoryEntryType, MemoryJournal};
use anyhow::Result;
use chrono::{DateTime, Utc};

/// Parse a memory journal from markdown content
pub fn parse_journal(content: &str, session_id: &str) -> Result<MemoryJournal> {
    let mut journal = MemoryJournal {
        session_id: session_id.to_string(),
        ..Default::default()
    };

    let mut current_entry: Option<EntryBuilder> = None;

    for line in content.lines() {
        // Skip header lines and separators
        if line.starts_with("<!--") || line.starts_with("# Memory Journal") || line == "---" {
            // Save previous entry if any
            if let Some(builder) = current_entry.take() {
                if let Some(entry) = builder.build() {
                    journal.entries.push(entry);
                }
            }
            continue;
        }

        // Parse stage ID from header
        if line.starts_with("**Stage**:") {
            journal.stage_id = Some(line.trim_start_matches("**Stage**:").trim().to_string());
            continue;
        }

        // Parse summary marker
        if line.starts_with("## Summary") {
            // Save previous entry if any
            if let Some(builder) = current_entry.take() {
                if let Some(entry) = builder.build() {
                    journal.entries.push(entry);
                }
            }
            continue;
        }

        // Detect entry header: ### 📝 Note [HH:MM:SS]
        if line.starts_with("### ") {
            // Save previous entry if any
            if let Some(builder) = current_entry.take() {
                if let Some(entry) = builder.build() {
                    journal.entries.push(entry);
                }
            }

            // Parse entry header
            let header = line.trim_start_matches("### ");
            if let Some((type_part, time_part)) = header.split_once('[') {
                let time_str = time_part.trim_end_matches(']').trim();
                let type_str = type_part.trim();

                let entry_type = if type_str.contains("Note") {
                    MemoryEntryType::Note
                } else if type_str.contains("Decision") {
                    MemoryEntryType::Decision
                } else if type_str.contains("Question") {
                    MemoryEntryType::Question
                } else {
                    continue;
                };

                // Parse time (just use current date with parsed time)
                let timestamp = chrono::NaiveTime::parse_from_str(time_str, "%H:%M:%S")
                    .ok()
                    .map(|t| {
                        let today = Utc::now().date_naive();
                        DateTime::from_naive_utc_and_offset(today.and_time(t), Utc)
                    })
                    .unwrap_or_else(Utc::now);

                current_entry = Some(EntryBuilder {
                    timestamp,
                    entry_type,
                    content: String::new(),
                    context: None,
                    in_context: false,
                });
            }
            continue;
        }

        // Parse content for current entry
        if let Some(builder) = &mut current_entry {
            if line.starts_with("*Context:*") {
                let ctx = line.trim_start_matches("*Context:*").trim().to_string();
                builder.context = Some(ctx);
                builder.in_context = true;
            } else if !line.is_empty() && !builder.in_context {
                if !builder.content.is_empty() {
                    builder.content.push('\n');
                }
                builder.content.push_str(line);
            }
        }
    }

    // Save last entry
    if let Some(builder) = current_entry {
        if let Some(entry) = builder.build() {
            journal.entries.push(entry);
        }
    }

    Ok(journal)
}

/// Format a memory entry for markdown output
pub fn format_entry(entry: &MemoryEntry) -> String {
    let mut output = String::new();

    let time = entry.timestamp.format("%H:%M:%S");
    let type_emoji = match entry.entry_type {
        MemoryEntryType::Note => "📝",
        MemoryEntryType::Decision => "✅",
        MemoryEntryType::Question => "❓",
    };

    output.push_str(&format!(
        "### {} {} [{}]\n\n",
        type_emoji,
        entry.entry_type.display_name(),
        time
    ));

    output.push_str(&entry.content);
    output.push_str("\n\n");

    if let Some(context) = &entry.context {
        output.push_str(&format!("*Context:* {context}\n\n"));
    }

    output.push_str("---\n\n");
    output
}
