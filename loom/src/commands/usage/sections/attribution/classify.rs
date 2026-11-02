//! Names the class of a tool result from the tool call that produced it.

use crate::commands::usage::transcript::ToolUse;

use super::super::reads::{is_knowledge_path, read_path};

const BUILD_TEST: [&str; 9] = [
    "cargo", "bun", "bunx", "pytest", "uv", "go", "make", "tsc", "eslint",
];
const SEARCH: [&str; 5] = ["rg", "grep", "fd", "find", "ls"];
const FILE_READ: [&str; 7] = ["sed", "cat", "head", "tail", "awk", "less", "more"];

/// `tool` is `None` when the call is outside the retained window.
pub(super) fn tool_result(tool: Option<&ToolUse>) -> String {
    let Some(tool) = tool else {
        return "other-tool".to_owned();
    };
    let class = match tool.name.as_str() {
        "Read" => read_class(tool),
        "Bash" => bash_class(tool),
        "Agent" | "Task" => "agent-report",
        "Skill" => "skill-tool",
        "Edit" | "Write" | "MultiEdit" => "edit-ack",
        "Grep" | "Glob" => "search-tool",
        _ => "other-tool",
    };
    class.to_owned()
}

fn read_class(tool: &ToolUse) -> &'static str {
    let path = read_path(tool).unwrap_or_default();
    if path.contains("loom-skill-catalog") || path.ends_with("SKILL.md") {
        "read:skill"
    } else if is_knowledge_path(&path) {
        "read:knowledge"
    } else if is_plan_or_signal(&path) {
        "read:plan-or-signal"
    } else {
        "read:source-other"
    }
}

fn is_plan_or_signal(path: &str) -> bool {
    ["doc/plans/", ".loom/", "signals/"]
        .iter()
        .any(|part| path.starts_with(part) || path.contains(&format!("/{part}")))
}

fn bash_class(tool: &ToolUse) -> &'static str {
    let command = tool
        .input
        .get("command")
        .and_then(|command| command.as_str())
        .unwrap_or_default();
    let word = leading_word(command);
    if BUILD_TEST.contains(&word) {
        "bash:build-test"
    } else if word == "loom" {
        "bash:loom"
    } else if word == "git" {
        "bash:git"
    } else if SEARCH.contains(&word) {
        "bash:search"
    } else if FILE_READ.contains(&word) {
        "bash:file-read"
    } else {
        "bash:other"
    }
}

/// The command word, after any leading `cd <dir> &&`.
fn leading_word(command: &str) -> &str {
    let mut rest = command.trim_start();
    while rest.starts_with("cd ") {
        match rest.split_once("&&") {
            Some((_, after)) => rest = after.trim_start(),
            None => break,
        }
    }
    rest.split_whitespace().next().unwrap_or_default()
}
