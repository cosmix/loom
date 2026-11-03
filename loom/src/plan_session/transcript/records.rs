//! Record classification: which transcript records map to what, and the tool item they build.
//!
//! Record bodies are untrusted: nothing here panics on a malformed record, and every string
//! that reaches the chat model is capped in characters on a char boundary.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{tools, Subagent};
use crate::plan_session::model::{Item, ToolStatus};

/// Longest record uuid, message id, tool use id or agent id accepted.
pub(super) const ID_CHARS: usize = 128;
/// Longest markdown text or pretty-printed tool input kept on an item.
pub(super) const ITEM_CHARS: usize = 64 * 1024;
/// Longest user text handed to `parse_unwrapped` (a delivered message is at most 100 KB).
pub(super) const USER_TEXT_CHARS: usize = 100_000;
/// Longest notice text or `error_line`.
const LINE_CHARS: usize = 500;
/// Longest tool name, and longest `filePath` accepted as a changed path.
const NAME_CHARS: usize = 200;
const PATH_CHARS: usize = 4096;

const INTERRUPT_PREFIX: &str = "[Request interrupted by user";
const TASK_NOTIFICATION: &str = "<task-notification>";
const WRAPPER_TAGS: [&str; 5] = [
    "<command-",
    "<local-command-",
    "<bash-",
    TASK_NOTIFICATION,
    "<system-reminder>",
];

pub(super) struct Record {
    pub(super) uuid: String,
    pub(super) at: Option<DateTime<Utc>>,
    pub(super) kind: Kind,
}

pub(super) enum Kind {
    Assistant(Assistant),
    ToolResults {
        results: Vec<ToolResult>,
        tool_use_result: Value,
        interrupted: bool,
    },
    Interrupt,
    /// The delivered text of a human prompt, attachment block included.
    Human(String),
    SelfStarted,
    TurnDuration(u64),
    Compacted,
    AgentsKilled,
    /// The first line of the error.
    ApiError(String),
    Skipped,
}

pub(super) struct Assistant {
    pub(super) message_id: Option<String>,
    pub(super) usage: Option<Value>,
    pub(super) blocks: Vec<Block>,
    pub(super) end_turn: bool,
}

pub(super) enum Block {
    Text(String),
    Thinking,
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    Other,
}

pub(super) struct ToolResult {
    pub(super) tool_use_id: String,
    pub(super) content: Value,
    pub(super) is_error: bool,
}

/// The file a tool result changed (`toolUseResult.filePath`) and its patch.
pub(super) struct FileChange {
    pub(super) path: String,
    pub(super) counts: Option<(u32, u32)>,
    pub(super) patch: Option<String>,
}

/// One complete transcript line; `None` for invalid JSON or a record without a usable uuid.
pub(super) fn parse(line: &str) -> Option<Record> {
    let mut value: Value = serde_json::from_str(line).ok()?;
    let uuid = id(value.get("uuid"))?;
    let at = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc));
    let kind = match value.get("type").and_then(Value::as_str) {
        Some("assistant") => assistant(&mut value),
        Some("user") => user(&mut value),
        Some("system") => system(&value),
        _ => Kind::Skipped,
    };
    Some(Record { uuid, at, kind })
}

/// The `message.id` and `message.usage` of a line, for usage-only ingestion.
pub(super) fn usage_of(line: &str) -> Option<(String, Value)> {
    let mut value: Value = serde_json::from_str(line).ok()?;
    let message = value.get_mut("message")?;
    let message_id = id(message.get("id"))?;
    Some((message_id, message.get_mut("usage")?.take()))
}

fn id(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?;
    let fits = !text.is_empty() && text.chars().count() <= ID_CHARS;
    fits.then(|| text.to_owned())
}

fn assistant(value: &mut Value) -> Kind {
    let mut message = value
        .get_mut("message")
        .map(Value::take)
        .unwrap_or_default();
    if value.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
        let text = joined_text(&message["content"]);
        return Kind::ApiError(error_line(&text, "API error"));
    }
    let end_turn = message.get("stop_reason").and_then(Value::as_str) == Some("end_turn");
    let blocks = match message.get_mut("content").map(Value::take) {
        Some(Value::Array(blocks)) => blocks.into_iter().map(block).collect(),
        Some(Value::String(text)) => vec![Block::Text(text)],
        _ => Vec::new(),
    };
    Kind::Assistant(Assistant {
        message_id: id(message.get("id")),
        usage: message.get_mut("usage").map(Value::take),
        blocks,
        end_turn,
    })
}

fn block(mut block: Value) -> Block {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => match block.get_mut("text").map(Value::take) {
            Some(Value::String(text)) => Block::Text(text),
            _ => Block::Other,
        },
        Some("thinking" | "redacted_thinking") => Block::Thinking,
        Some("tool_use") => {
            let Some(id) = id(block.get("id")) else {
                return Block::Other;
            };
            let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
            let name = cap_chars(name, NAME_CHARS);
            let input = block.get_mut("input").map(Value::take).unwrap_or_default();
            Block::ToolUse { id, name, input }
        }
        _ => Block::Other,
    }
}

fn user(value: &mut Value) -> Kind {
    let content = value
        .get_mut("message")
        .and_then(|message| message.get_mut("content"))
        .map(Value::take)
        .unwrap_or_default();
    let texts = texts(&content);
    let interrupted = texts.iter().any(|text| text.starts_with(INTERRUPT_PREFIX));
    let text = texts.join("\n");
    let results = tool_results(content);
    if !results.is_empty() {
        let tool_use_result = value.get_mut("toolUseResult").map(Value::take);
        return Kind::ToolResults {
            results,
            tool_use_result: tool_use_result.unwrap_or_default(),
            interrupted,
        };
    }
    if interrupted {
        return Kind::Interrupt;
    }
    let origin = value.pointer("/origin/kind").and_then(Value::as_str);
    let lead = text.trim_start();
    if origin == Some("task-notification") || lead.starts_with(TASK_NOTIFICATION) {
        return Kind::SelfStarted;
    }
    let meta = value.get("isMeta").and_then(Value::as_bool) == Some(true);
    let foreign = origin.is_some_and(|kind| kind != "human");
    let wrapped = WRAPPER_TAGS.iter().any(|tag| lead.starts_with(tag));
    if meta || foreign || wrapped || lead.is_empty() {
        return Kind::Skipped;
    }
    Kind::Human(text)
}

/// A string content, or the text blocks of an array content.
fn texts(content: &Value) -> Vec<&str> {
    match content {
        Value::String(text) => vec![text.as_str()],
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

fn joined_text(content: &Value) -> String {
    texts(content).join("\n")
}

fn tool_results(content: Value) -> Vec<ToolResult> {
    let Value::Array(blocks) = content else {
        return Vec::new();
    };
    let result = |mut block: Value| {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            return None;
        }
        Some(ToolResult {
            tool_use_id: id(block.get("tool_use_id"))?,
            is_error: block.get("is_error").and_then(Value::as_bool) == Some(true),
            content: block
                .get_mut("content")
                .map(Value::take)
                .unwrap_or_default(),
        })
    };
    blocks.into_iter().filter_map(result).collect()
}

fn system(value: &Value) -> Kind {
    match value.get("subtype").and_then(Value::as_str) {
        Some("turn_duration") => value
            .get("durationMs")
            .and_then(Value::as_u64)
            .map_or(Kind::Skipped, Kind::TurnDuration),
        Some("compact_boundary") => Kind::Compacted,
        Some("agents_killed") => Kind::AgentsKilled,
        _ if is_api_error(value) => {
            let text = ["content", "apiError", "error"]
                .iter()
                .find_map(|key| describe(value.get(*key)))
                .unwrap_or("");
            Kind::ApiError(error_line(text, "API error"))
        }
        _ => Kind::Skipped,
    }
}

fn is_api_error(value: &Value) -> bool {
    let present = |key: &str| value.get(key).is_some_and(|field| !field.is_null());
    value.get("level").and_then(Value::as_str) == Some("error")
        || present("apiError")
        || present("error")
}

/// A non-blank string, or the `message` (else the nested `error`) of an object.
fn describe(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text),
        object @ Value::Object(_) => {
            describe(object.get("message")).or_else(|| describe(object.get("error")))
        }
        _ => None,
    }
}

/// The first non-blank line of `text`, capped; `fallback` when there is none.
fn error_line(text: &str, fallback: &str) -> String {
    first_line(text).unwrap_or_else(|| fallback.to_owned())
}

pub(super) fn first_line(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(cap_chars(line, LINE_CHARS))
}

/// `text` cut to at most `max` characters, on a char boundary.
pub(super) fn cap_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

pub(super) fn item_id(item: &Item) -> &str {
    match item {
        Item::User { id, .. }
        | Item::Thinking { id, .. }
        | Item::Text { id, .. }
        | Item::Tool { id, .. }
        | Item::Permission { id, .. }
        | Item::Notice { id, .. } => id,
    }
}

/// A pending tool item for a `tool_use` block; a subagent's row names its agent and the
/// spawning tool call.
pub(super) fn tool_item(
    id: String,
    tool: (String, String, &Value),
    root: &Path,
    sub: Option<Subagent<'_>>,
) -> Item {
    let (tool_use_id, name, input) = tool;
    let pretty = serde_json::to_string_pretty(input).unwrap_or_default();
    Item::Tool {
        id,
        tool_use_id,
        summary: tools::summary(&name, input, root),
        name,
        status: ToolStatus::Pending,
        subagent: sub.map(|sub| sub.agent_id.to_owned()),
        parent: sub.and_then(|sub| sub.spawned_by).map(str::to_owned),
        input: cap_chars(&pretty, ITEM_CHARS),
        result: None,
        result_truncated: false,
        error_line: None,
        added: None,
        removed: None,
        patch: None,
        nested_tools: 0,
    }
}

/// The changed file of a tool result, when `toolUseResult.filePath` names one.
pub(super) fn file_change(tool_use_result: &Value) -> Option<FileChange> {
    let path = tool_use_result.get("filePath")?.as_str()?;
    if path.is_empty() || path.chars().count() > PATH_CHARS {
        return None;
    }
    let patch = tool_use_result
        .get("structuredPatch")
        .filter(|patch| patch.is_array());
    Some(FileChange {
        path: path.to_owned(),
        counts: patch.map(tools::patch_counts),
        patch: patch
            .map(tools::unified_patch)
            .filter(|text| !text.is_empty()),
    })
}

/// Completes a tool item with its result; a denied tool keeps its status.
pub(super) fn complete_tool(item: &mut Item, outcome: &ToolResult, file: Option<&FileChange>) {
    let Item::Tool {
        status,
        result,
        result_truncated,
        error_line,
        added,
        removed,
        patch,
        ..
    } = item
    else {
        return;
    };
    let (text, truncated) = tools::result_text(&outcome.content);
    if *status != ToolStatus::Denied {
        *status = if outcome.is_error {
            ToolStatus::Error
        } else {
            ToolStatus::Done
        };
    }
    *error_line = if outcome.is_error {
        first_line(&text)
    } else {
        None
    };
    *result = Some(text);
    *result_truncated = truncated;
    if let Some(file) = file {
        if let Some((plus, minus)) = file.counts {
            (*added, *removed) = (Some(plus), Some(minus));
        }
        patch.clone_from(&file.patch);
    }
}
