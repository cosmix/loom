//! Type definitions for memory journal entries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

/// Builder for parsing memory entries
pub(crate) struct EntryBuilder {
    pub timestamp: DateTime<Utc>,
    pub entry_type: MemoryEntryType,
    pub content: String,
    pub context: Option<String>,
    pub in_context: bool,
}

impl EntryBuilder {
    pub fn build(self) -> Option<MemoryEntry> {
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
