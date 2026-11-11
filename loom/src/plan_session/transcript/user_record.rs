//! Classification of a transcript `type: "user"` record: tool results, interrupts, turns
//! Claude Code starts itself, records the chat never shows, and what the user wrote.

use serde_json::Value;

use super::records::{id, texts, Kind, ToolResult};

const INTERRUPT_PREFIX: &str = "[Request interrupted by user";
const TASK_NOTIFICATION: &str = "<task-notification>";
/// The lead of a teammate's message, delivered as a user record that starts a turn.
const TEAMMATE_LEAD: &str = "Another Claude session sent a message:";
/// The leads of the prompts Claude Code submits itself (probe P1): a background task's
/// `<task-notification>`, an agent's `<agent-message>` hand-back and a teammate's message.
pub(crate) const BACKGROUND_TAGS: [&str; 3] = [TASK_NOTIFICATION, "<agent-message", TEAMMATE_LEAD];
const WRAPPER_TAGS: [&str; 5] = [
    "<command-",
    "<local-command-",
    "<bash-",
    TASK_NOTIFICATION,
    "<system-reminder>",
];

pub(super) fn user(value: &mut Value) -> Kind {
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
    if origin == Some("task-notification")
        || lead.starts_with(TASK_NOTIFICATION)
        || lead.starts_with(TEAMMATE_LEAD)
    {
        return Kind::SelfStarted;
    }
    if hidden(value, origin, lead) {
        return Kind::Skipped;
    }
    Kind::Human(text)
}

/// A user record the chat never shows: a meta or wrapper record, one a peer wrote, a
/// compaction summary (`compact_boundary` already produces the notice), or an empty one.
fn hidden(value: &Value, origin: Option<&str>, lead: &str) -> bool {
    let flag = |key: &str| value.get(key).and_then(Value::as_bool) == Some(true);
    let foreign = origin.is_some_and(|kind| kind != "human");
    let wrapped = WRAPPER_TAGS.iter().any(|tag| lead.starts_with(tag));
    flag("isMeta")
        || flag("isCompactSummary")
        || flag("isVisibleInTranscriptOnly")
        || foreign
        || wrapped
        || lead.is_empty()
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
