//! The `### Suggestions` section of memory summaries, signals and handoffs.

use crate::fs::memory::query::entries_of_type;
use crate::fs::memory::{MemoryEntry, MemoryEntryType};
use crate::utils::truncate_for_display;

/// Append a `### Suggestions` section listing every suggestion among
/// `entries` with the id `loom memory resolve` takes, its content truncated
/// to `max_chars`. Appends nothing when there is no suggestion.
pub(in crate::fs::memory) fn push_suggestions_section<'a>(
    output: &mut String,
    entries: impl IntoIterator<Item = &'a MemoryEntry>,
    max_chars: usize,
) {
    let suggestions = entries_of_type(entries, MemoryEntryType::Suggestion);
    if suggestions.is_empty() {
        return;
    }
    output.push_str("### Suggestions\n\n");
    for entry in suggestions {
        output.push_str(&format!(
            "- **[{}]** `{}` {}\n",
            entry.timestamp.format("%H:%M"),
            entry.id,
            truncate_for_display(&entry.content, max_chars)
        ));
    }
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use crate::fs::memory::{
        append_entry, format_memory_for_handoff, format_memory_for_signal, generate_summary,
        MemoryEntry, MemoryEntryType, MemoryJournal,
    };

    #[test]
    fn summary_signal_and_handoff_list_suggestions_with_their_ids() {
        let temp = tempfile::tempdir().unwrap();
        let suggestion = MemoryEntry::new(MemoryEntryType::Suggestion, "tidy it".to_string());
        let note = MemoryEntry::new(MemoryEntryType::Note, "plain note".to_string());
        append_entry(temp.path(), "stage-a", &note).unwrap();
        append_entry(temp.path(), "stage-a", &suggestion).unwrap();
        let journal = MemoryJournal {
            stage_id: "stage-a".to_string(),
            entries: vec![note, suggestion.clone()],
            summary: None,
        };
        let line = format!("`{}` tidy it", suggestion.id);

        for rendered in [
            generate_summary(&journal, 5),
            format_memory_for_signal(temp.path(), "stage-a", 10).unwrap(),
            format_memory_for_handoff(temp.path(), "stage-a").unwrap(),
        ] {
            let after_heading = rendered.split("### Suggestions\n\n").nth(1).unwrap();
            let section = after_heading.split("### ").next().unwrap();
            assert!(section.contains(&line), "{rendered}");
            assert!(!section.contains("plain note"), "{rendered}");
        }
    }
}
