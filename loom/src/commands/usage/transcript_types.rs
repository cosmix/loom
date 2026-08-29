//! Record and entry type definitions for parsed Claude Code transcripts.
//!
//! Split out of `transcript` to keep that module under the 400-line cap:
//! this owns the shapes parsing produces, `transcript` owns turning JSONL
//! lines into them.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Main,
    Subagent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub ephemeral_5m: u64,
    pub ephemeral_1h: u64,
}

impl TokenUsage {
    /// Tokens the model had resident for this request.
    pub fn resident(&self) -> u64 {
        self.input + self.cache_creation + self.cache_read
    }
    /// Field-wise sum, for rolling totals.
    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.cache_creation += other.cache_creation;
        self.cache_read += other.cache_read;
        self.output += other.output;
        self.ephemeral_5m += other.ephemeral_5m;
        self.ephemeral_1h += other.ephemeral_1h;
    }
}

#[derive(Debug, Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub message_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub model: String,
    pub usage: TokenUsage,
    pub tool_uses: Vec<ToolUse>,
    pub thinking_chars: usize,
    pub text_chars: usize,
}

#[derive(Debug, Clone)]
pub struct UserEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// `Some` when this record is a `tool_result`, naming the `tool_use` it
    /// answers; `None` when it is a message sent to the agent.
    pub tool_use_id: Option<String>,
    /// The tool result's text, or the message text.
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Entry {
    Assistant(Request),
    User(UserEntry),
}

impl Entry {
    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::Assistant(request) => request.timestamp,
            Self::User(entry) => entry.timestamp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub path: std::path::PathBuf,
    pub scope: Scope,
    pub project_slug: String,
    /// Main session UUID. For a subagent this is its PARENT session's UUID -
    /// that is what makes the parent/child tree the report needs.
    pub session_id: String,
    pub agent_id: Option<String>,
    /// The transcript's first user entry, captured BEFORE the `since` cutoff
    /// is applied. Spawn-prompt classification reads it, and a cutoff that
    /// dropped it would silently reclassify every long-running subagent from
    /// its real preamble to whatever mid-conversation message survived.
    pub first_user_entry: Option<UserEntry>,
    /// File order, deduplicated, entries older than the `since` cutoff dropped.
    pub entries: Vec<Entry>,
}

impl Transcript {
    pub fn requests(&self) -> impl Iterator<Item = &Request> {
        self.entries.iter().filter_map(|entry| match entry {
            Entry::Assistant(request) => Some(request),
            Entry::User(_) => None,
        })
    }

    pub fn total_usage(&self) -> TokenUsage {
        self.requests()
            .fold(TokenUsage::default(), |mut total, request| {
                total.add(&request.usage);
                total
            })
    }
}
