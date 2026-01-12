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

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Entry type in the memory journal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryEntryType {
    /// General observations and context
    Note,
    /// Choices made with rationale
    Decision,
    /// Open questions for future investigation
    Question,
}

impl MemoryEntryType {
    /// Get a display name for this entry type
    pub fn display_name(&self) -> &'static str {
        match self {
            MemoryEntryType::Note => "Note",
            MemoryEntryType::Decision => "Decision",
            MemoryEntryType::Question => "Question",
        }
    }

    /// Get all entry types
    pub fn all() -> &'static [MemoryEntryType] {
        &[
            MemoryEntryType::Note,
            MemoryEntryType::Decision,
            MemoryEntryType::Question,
        ]
    }
}

impl std::fmt::Display for MemoryEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryEntryType::Note => write!(f, "note"),
            MemoryEntryType::Decision => write!(f, "decision"),
            MemoryEntryType::Question => write!(f, "question"),
        }
    }
}

impl std::str::FromStr for MemoryEntryType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "note" | "notes" => Ok(MemoryEntryType::Note),
            "decision" | "decisions" => Ok(MemoryEntryType::Decision),
            "question" | "questions" => Ok(MemoryEntryType::Question),
            _ => anyhow::bail!("Invalid entry type: {s}. Use: note, decision, question"),
        }
    }
}

/// A single memory entry in the journal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// When the entry was recorded
    pub timestamp: DateTime<Utc>,
    /// Type of entry
    pub entry_type: MemoryEntryType,
    /// The content of the entry
    pub content: String,
    /// Optional additional context or rationale (for decisions)
    pub context: Option<String>,
}

impl MemoryEntry {
    /// Create a new memory entry
    pub fn new(entry_type: MemoryEntryType, content: String) -> Self {
        Self {
            timestamp: Utc::now(),
            entry_type,
            content,
            context: None,
        }
    }

    /// Create a new memory entry with context
    pub fn with_context(entry_type: MemoryEntryType, content: String, context: String) -> Self {
        Self {
            timestamp: Utc::now(),
            entry_type,
            content,
            context: Some(context),
        }
    }
}

/// Memory journal for a session
#[derive(Debug, Clone, Default)]
pub struct MemoryJournal {
    /// Session ID this journal belongs to
    pub session_id: String,
    /// Stage ID associated with this session
    pub stage_id: Option<String>,
    /// All entries in the journal
    pub entries: Vec<MemoryEntry>,
    /// Summary of the journal (generated at context threshold)
    pub summary: Option<String>,
}

/// Header for a memory journal file
const MEMORY_HEADER: &str = "<!-- loom-memory-journal -->\n";

/// Get the memory directory path
pub fn memory_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("memory")
}

/// Get the path to a session's memory file
pub fn memory_file_path(work_dir: &Path, session_id: &str) -> PathBuf {
    memory_dir(work_dir).join(format!("{session_id}.md"))
}

/// Initialize the memory directory
pub fn init_memory_dir(work_dir: &Path) -> Result<()> {
    let memory_path = memory_dir(work_dir);

    if !memory_path.exists() {
        fs::create_dir_all(&memory_path).with_context(|| {
            format!(
                "Failed to create memory directory: {}",
                memory_path.display()
            )
        })?;
    }

    Ok(())
}

/// Create a new memory journal for a session
pub fn create_journal(
    work_dir: &Path,
    session_id: &str,
    stage_id: Option<&str>,
) -> Result<PathBuf> {
    init_memory_dir(work_dir)?;

    let file_path = memory_file_path(work_dir, session_id);
    let stage_line = stage_id
        .map(|s| format!("**Stage**: {s}\n"))
        .unwrap_or_default();

    let header = format!(
        "{MEMORY_HEADER}# Memory Journal: {session_id}\n\n**Session**: {session_id}\n{stage_line}**Created**: {}\n\n---\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    fs::write(&file_path, header)
        .with_context(|| format!("Failed to create memory journal: {}", file_path.display()))?;

    Ok(file_path)
}

/// Append an entry to a session's memory journal
pub fn append_entry(work_dir: &Path, session_id: &str, entry: &MemoryEntry) -> Result<()> {
    let file_path = memory_file_path(work_dir, session_id);

    // Create journal if it doesn't exist
    if !file_path.exists() {
        create_journal(work_dir, session_id, None)?;
    }

    let formatted = format_entry(entry);

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .with_context(|| format!("Failed to open memory journal: {}", file_path.display()))?;

    file.write_all(formatted.as_bytes()).with_context(|| {
        format!(
            "Failed to append to memory journal: {}",
            file_path.display()
        )
    })?;

    Ok(())
}

/// Format a memory entry for markdown output
fn format_entry(entry: &MemoryEntry) -> String {
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

/// Read a session's memory journal
pub fn read_journal(work_dir: &Path, session_id: &str) -> Result<MemoryJournal> {
    let file_path = memory_file_path(work_dir, session_id);

    if !file_path.exists() {
        return Ok(MemoryJournal {
            session_id: session_id.to_string(),
            ..Default::default()
        });
    }

    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read memory journal: {}", file_path.display()))?;

    parse_journal(&content, session_id)
}

/// Parse a memory journal from markdown content
fn parse_journal(content: &str, session_id: &str) -> Result<MemoryJournal> {
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

/// Builder for parsing memory entries
struct EntryBuilder {
    timestamp: DateTime<Utc>,
    entry_type: MemoryEntryType,
    content: String,
    context: Option<String>,
    in_context: bool,
}

impl EntryBuilder {
    fn build(self) -> Option<MemoryEntry> {
        if self.content.is_empty() {
            return None;
        }

        Some(MemoryEntry {
            timestamp: self.timestamp,
            entry_type: self.entry_type,
            content: self.content.trim().to_string(),
            context: self.context,
        })
    }
}

/// Get recent entries from a journal (for recitation in signals)
pub fn get_recent_entries(journal: &MemoryJournal, max_entries: usize) -> Vec<&MemoryEntry> {
    let len = journal.entries.len();
    if len <= max_entries {
        journal.entries.iter().collect()
    } else {
        journal.entries[(len - max_entries)..].iter().collect()
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

/// Query memory entries by search term
pub fn query_entries<'a>(journal: &'a MemoryJournal, search: &str) -> Vec<&'a MemoryEntry> {
    let search_lower = search.to_lowercase();
    journal
        .entries
        .iter()
        .filter(|e| {
            e.content.to_lowercase().contains(&search_lower)
                || e.context
                    .as_ref()
                    .is_some_and(|c| c.to_lowercase().contains(&search_lower))
        })
        .collect()
}

/// Generate a summary of the memory journal (for context threshold)
pub fn generate_summary(journal: &MemoryJournal, max_entries: usize) -> String {
    let mut summary = String::new();
    summary.push_str("## Summary\n\n");
    summary.push_str("Auto-generated summary at context threshold.\n\n");

    let notes: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| e.entry_type == MemoryEntryType::Note)
        .collect();
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

    summary.push_str(&format!("- **Total entries**: {}\n", journal.entries.len()));
    summary.push_str(&format!("- **Notes**: {}\n", notes.len()));
    summary.push_str(&format!("- **Decisions**: {}\n", decisions.len()));
    summary.push_str(&format!("- **Questions**: {}\n\n", questions.len()));

    // Key decisions (most recent)
    if !decisions.is_empty() {
        summary.push_str("### Key Decisions\n\n");
        for entry in decisions.iter().rev().take(max_entries) {
            summary.push_str(&format!("- {}\n", truncate_content(&entry.content, 200)));
        }
        summary.push('\n');
    }

    // Open questions (all)
    if !questions.is_empty() {
        summary.push_str("### Open Questions\n\n");
        for entry in &questions {
            summary.push_str(&format!("- {}\n", truncate_content(&entry.content, 200)));
        }
        summary.push('\n');
    }

    summary
}

/// Write summary to the journal file
pub fn write_summary(work_dir: &Path, session_id: &str, summary: &str) -> Result<()> {
    let file_path = memory_file_path(work_dir, session_id);

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .with_context(|| format!("Failed to open memory journal: {}", file_path.display()))?;

    file.write_all(summary.as_bytes()).with_context(|| {
        format!(
            "Failed to write summary to memory journal: {}",
            file_path.display()
        )
    })?;

    Ok(())
}

/// Truncate content for display
fn truncate_content(s: &str, max_len: usize) -> String {
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    if single_line.len() <= max_len {
        single_line
    } else {
        format!("{}…", &single_line[..max_len - 1])
    }
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

/// Copy memory journal to crash recovery location
pub fn preserve_for_crash(work_dir: &Path, session_id: &str) -> Result<Option<PathBuf>> {
    let source = memory_file_path(work_dir, session_id);

    if !source.exists() {
        return Ok(None);
    }

    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        fs::create_dir_all(&crashes_dir)?;
    }

    let dest = crashes_dir.join(format!("memory-{session_id}.md"));
    fs::copy(&source, &dest)
        .with_context(|| format!("Failed to preserve memory for crash: {}", source.display()))?;

    Ok(Some(dest))
}

/// Extract key notes from memory to promote to learnings on completion
pub fn extract_key_notes(journal: &MemoryJournal) -> Vec<String> {
    let mut key_notes = Vec::new();

    // Extract all decisions as potential learnings
    for entry in &journal.entries {
        if entry.entry_type == MemoryEntryType::Decision {
            let note = if let Some(ctx) = &entry.context {
                format!("{} ({})", entry.content, ctx)
            } else {
                entry.content.clone()
            };
            key_notes.push(note);
        }
    }

    key_notes
}

/// Validate memory entry content
pub fn validate_content(content: &str) -> Result<()> {
    if content.is_empty() {
        anyhow::bail!("Memory entry content cannot be empty");
    }

    if content.len() > 2000 {
        anyhow::bail!(
            "Memory entry content too long: {} characters (max 2000)",
            content.len()
        );
    }

    Ok(())
}

/// List all memory journals in the work directory
pub fn list_journals(work_dir: &Path) -> Result<Vec<String>> {
    let memory_path = memory_dir(work_dir);

    if !memory_path.exists() {
        return Ok(Vec::new());
    }

    let mut journals = Vec::new();

    for entry in fs::read_dir(&memory_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(stem) = path.file_stem() {
                journals.push(stem.to_string_lossy().to_string());
            }
        }
    }

    Ok(journals)
}

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

        let content = fs::read_to_string(&preserved).unwrap();
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
}
