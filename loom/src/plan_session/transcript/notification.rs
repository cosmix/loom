//! Background-task evidence in main transcript records (probe P1 and P2,
//! `tests/fixtures/plan_session/probe-notes.md`): the `<task-notification>` body, the launch
//! results of background agents and shells, a `TaskStop` result, an agent's hand-back and a fork
//! skill's `<forked-skill-launch>` record.
//!
//! Record bodies are untrusted: nothing here panics on a malformed record, every id is bounded
//! as the mapper bounds ids, and every kept string is capped.

use serde_json::Value;

use super::records::{bounded_id, cap_chars, id, LINE_CHARS};
use crate::agent_state::background_task_id;

const NOTIFICATION: &str = "<task-notification>";
const FORK_OPEN: &str = "<forked-skill-launch>";
const FORK_CLOSE: &str = "</forked-skill-launch>";
/// The summary ending of the interim agent notification: the agent stopped while a task of its
/// own still runs, and the same task id notifies again if it resumes.
const INTERIM: &str = "is waiting on its own background work";
/// The tools whose call `description` labels the task the call launches.
const LAUNCHERS: [&str; 3] = ["Agent", "Task", "Bash"];

/// A parsed `<task-notification>` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNotification {
    pub task_id: String,
    /// The launching call; a fork skill's notification has none.
    pub tool_use_id: Option<String>,
    pub status: NotificationStatus,
    pub summary: Option<String>,
    /// The agent stopped while a task of its own still runs.
    pub interim: bool,
}

/// The `<status>` element of a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationStatus {
    Completed,
    Failed,
    Killed,
    Other(String),
}

/// What one main transcript record says about a background task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundEvidence {
    /// An `Agent` or `Task` result with `status: "async_launched"`.
    AgentLaunch {
        agent_id: String,
        tool_use_id: Option<String>,
        label: Option<String>,
    },
    /// A `Bash` result carrying `backgroundTaskId`.
    ShellLaunch {
        task_id: String,
        tool_use_id: Option<String>,
        label: Option<String>,
    },
    /// A fork skill's launch, from its `local_command` system record.
    ForkLaunch {
        agent_id: String,
        label: Option<String>,
    },
    /// A `TaskStop` result naming the task it stopped.
    TaskStopped {
        task_id: String,
    },
    Notification(TaskNotification),
    /// A finished agent's hand-back user record.
    Handback {
        agent_id: String,
        report: Option<String>,
    },
}

impl BackgroundEvidence {
    /// A launch without a label takes the one `describe` gives its calling `tool_use_id`.
    pub(super) fn labelled(mut self, describe: impl FnOnce(&str) -> Option<String>) -> Self {
        if let Self::AgentLaunch {
            tool_use_id: Some(call),
            label: label @ None,
            ..
        }
        | Self::ShellLaunch {
            tool_use_id: Some(call),
            label: label @ None,
            ..
        } = &mut self
        {
            *label = describe(call);
        }
        self
    }
}

/// The body that starts at the first `<task-notification>` of `text`. The status comes from
/// the `<status>` element only, never from the summary wording; a body without a `<task-id>`
/// or a `<status>` is not parsed. Each element is read up to its own closing tag, so a redacted
/// fixture id such as `<AGENT-1>` stays whole.
pub fn parse_task_notification(text: &str) -> Option<TaskNotification> {
    let start = text.find(NOTIFICATION)? + NOTIFICATION.len();
    let body = &text[start..];
    let task_id = bounded_id(element(body, "task-id")?)?;
    let status = match element(body, "status")? {
        "completed" => NotificationStatus::Completed,
        "failed" => NotificationStatus::Failed,
        "killed" => NotificationStatus::Killed,
        other => NotificationStatus::Other(cap_chars(other, LINE_CHARS)),
    };
    let summary = element(body, "summary").filter(|summary| !summary.is_empty());
    Some(TaskNotification {
        task_id,
        tool_use_id: element(body, "tool-use-id").and_then(bounded_id),
        status,
        interim: summary.is_some_and(|summary| summary.ends_with(INTERIM)),
        summary: summary.map(|summary| cap_chars(summary, LINE_CHARS)),
    })
}

/// The trimmed text between the first `<tag>` and the `</tag>` after it.
fn element<'a>(body: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&format!("</{tag}>"))? + start;
    Some(body[start..end].trim())
}

/// The capped `description` of an `Agent`, `Task` or `Bash` call, which labels what it launches.
pub(super) fn launch_label(name: &str, input: &Value) -> Option<String> {
    let description = LAUNCHERS
        .contains(&name)
        .then(|| input.get("description").and_then(Value::as_str))
        .flatten()?;
    let description = description.trim();
    (!description.is_empty()).then(|| cap_chars(description, LINE_CHARS))
}

/// The background evidence of one transcript record, before the mapper consumes it.
pub(super) fn evidence(record: &Value) -> Option<BackgroundEvidence> {
    match record.get("type").and_then(Value::as_str)? {
        "user" => user_evidence(record),
        "system" => fork_launch(record),
        _ => None,
    }
}

fn user_evidence(record: &Value) -> Option<BackgroundEvidence> {
    let origin = record.get("origin");
    let kind = origin
        .and_then(|origin| origin.get("kind"))
        .and_then(Value::as_str);
    let flag = |key: &str| origin.and_then(|origin| origin.get(key)?.as_bool()) == Some(true);
    if kind == Some("peer") && flag("handback") {
        return handback(origin?);
    }
    let content = record.pointer("/message/content")?;
    if let Some(blocks) = content.as_array().filter(|blocks| has_tool_result(blocks)) {
        return tool_evidence(blocks, record.get("toolUseResult")?);
    }
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => texts(blocks),
        _ => return None,
    };
    let notified = kind == Some("task-notification") || text.trim_start().starts_with(NOTIFICATION);
    notified
        .then(|| parse_task_notification(&text))
        .flatten()
        .map(BackgroundEvidence::Notification)
}

fn handback(origin: &Value) -> Option<BackgroundEvidence> {
    let agent_id = id(origin.get("senderTaskId"))?;
    let report = origin
        .get("handbackReportText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|report| !report.is_empty())
        .map(|report| cap_chars(report, LINE_CHARS));
    Some(BackgroundEvidence::Handback { agent_id, report })
}

fn has_tool_result(blocks: &[Value]) -> bool {
    blocks.iter().any(is_tool_result)
}

fn is_tool_result(block: &Value) -> bool {
    block.get("type").and_then(Value::as_str) == Some("tool_result")
}

fn texts(blocks: &[Value]) -> String {
    let text = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str));
    text.collect::<Vec<_>>().join("\n")
}

/// A launch or `TaskStop` result. `toolUseResult` describes the record's one result, so the
/// result's `tool_use_id` is kept only when the record holds exactly one.
fn tool_evidence(blocks: &[Value], result: &Value) -> Option<BackgroundEvidence> {
    let mut calls = blocks
        .iter()
        .filter(|block| is_tool_result(block))
        .map(|block| id(block.get("tool_use_id")));
    let tool_use_id = match (calls.next(), calls.next()) {
        (Some(call), None) => call,
        _ => None,
    };
    let text = |key: &str| result.get(key).and_then(Value::as_str);
    if text("status") == Some("async_launched") {
        let agent_id = id(result.get("agentId"))?;
        let label = None;
        return Some(BackgroundEvidence::AgentLaunch {
            agent_id,
            tool_use_id,
            label,
        });
    }
    if let Some(task_id) = background_task_id(result) {
        let label = None;
        return Some(BackgroundEvidence::ShellLaunch {
            task_id,
            tool_use_id,
            label,
        });
    }
    let stopped = matches!(text("task_type"), Some("local_bash" | "local_agent"));
    let task_id = stopped.then(|| id(result.get("task_id"))).flatten()?;
    Some(BackgroundEvidence::TaskStopped { task_id })
}

/// `<forked-skill-launch>{"agentId", "skillName", "description"}</forked-skill-launch>` in a
/// `local_command` system record's content.
fn fork_launch(record: &Value) -> Option<BackgroundEvidence> {
    if record.get("subtype").and_then(Value::as_str) != Some("local_command") {
        return None;
    }
    let content = record.get("content").and_then(Value::as_str)?;
    let start = content.find(FORK_OPEN)? + FORK_OPEN.len();
    let end = content[start..].find(FORK_CLOSE)? + start;
    let launch: Value = serde_json::from_str(&content[start..end]).ok()?;
    let agent_id = id(launch.get("agentId"))?;
    let label = launch
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| cap_chars(label, LINE_CHARS));
    Some(BackgroundEvidence::ForkLaunch { agent_id, label })
}

#[cfg(test)]
#[path = "notification_tests.rs"]
mod tests;
