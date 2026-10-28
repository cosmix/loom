//! The chat model: the session view, turns, items, deltas and snapshots the bridge publishes. Pure data.

use serde::{Deserialize, Serialize};

use super::attachments::Attachment;

/// The current state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Idle,
    Working,
    Permission,
    Ending,
    Ended,
    Failed,
}

/// The activity tracked underneath the ending state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Idle,
    Working,
    Permission,
}

/// The session details shown in chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionView {
    pub id: String,
    pub kind: String,
    pub state: SessionState,
    pub name: String,
    pub model: String,
    pub socket: String,
    pub holder: Option<String>,
    pub away_until: Option<String>,
    pub queued: u32,
    pub started_at: String,
    pub state_since: String,
    pub working_since: Option<String>,
    pub subagents_running: u32,
    pub last_activity: Option<String>,
    pub failure: Option<Failure>,
    pub offer: Option<String>,
}

/// A session failure and its explanation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}

/// The cause of a session failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Spawn,
    Died,
}

/// Token usage accumulated for a turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
}

/// A file changed during a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub before_hash: Option<String>,
    pub by_tool: bool,
}

/// A chat turn and its contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub open: bool,
    pub tokens: TokenCounts,
    pub items: Vec<Item>,
    pub changed_files: Vec<ChangedFile>,
    pub summaries_rebuilding: Vec<String>,
}

/// A message, tool call, permission request, or notice within a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    User {
        id: String,
        text: String,
        attachments: Vec<Attachment>,
        delivery: Delivery,
        origin: Option<Origin>,
    },
    Thinking {
        id: String,
        duration_ms: Option<u64>,
    },
    Text {
        id: String,
        markdown: String,
    },
    Tool {
        id: String,
        tool_use_id: String,
        name: String,
        summary: String,
        status: ToolStatus,
        subagent: Option<String>,
        /// A subagent row's spawning tool call (its `tool_use_id`); `None` for the main agent.
        parent: Option<String>,
        input: String,
        result: Option<String>,
        result_truncated: bool,
        error_line: Option<String>,
        added: Option<u32>,
        removed: Option<u32>,
        patch: Option<String>,
        nested_tools: u32,
    },
    Permission {
        id: String,
        request_id: String,
        tool: String,
        input: String,
        status: PermissionStatus,
        deny_reason: Option<DenyReason>,
        note: Option<String>,
        expires_at: String,
    },
    Notice {
        id: String,
        kind: NoticeKind,
        text: String,
    },
}

/// The delivery state of a user message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    Sent,
    Delivered,
    Undelivered,
}

/// The source of a generated user message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Offer,
    Start,
    Closing,
}

/// The execution state of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    Done,
    Error,
    Denied,
    NotFinished,
}

/// The resolution state of a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Pending,
    Allowed,
    Denied,
}

/// The reason a permission request was denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    Holder,
    Timeout,
    SessionEnded,
    ServerStopping,
    HookError,
    Interrupt,
    SummarySession,
}

/// The category of a chat notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Interrupted,
    Compacted,
    ApiError,
    Stopped,
    Info,
}

/// A message waiting for delivery, with full text visible to the holder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub id: String,
    pub first_line: String,
    pub text: Option<String>,
}

/// A completed chat retained alongside the current session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndedChat {
    pub session: SessionView,
    pub turns: Vec<Turn>,
}

/// The complete chat state sent to a browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatSnapshot {
    pub session: Option<SessionView>,
    pub turns: Vec<Turn>,
    pub queue: Vec<QueuedMessage>,
    pub previous: Option<EndedChat>,
}

/// An incremental change to the chat state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChatDelta {
    Session {
        session: SessionView,
    },
    TurnAdded {
        turn: Turn,
    },
    TurnUpdated {
        turn_id: String,
        duration_ms: Option<u64>,
        open: bool,
        tokens: TokenCounts,
        changed_files: Vec<ChangedFile>,
        summaries_rebuilding: Vec<String>,
    },
    ItemAdded {
        turn_id: String,
        item: Item,
    },
    ItemReplaced {
        turn_id: String,
        item: Item,
    },
    ItemRemoved {
        turn_id: String,
        id: String,
    },
    Queue {
        queue: Vec<QueuedMessage>,
    },
    Cleared,
}

#[cfg(test)]
mod tests;
