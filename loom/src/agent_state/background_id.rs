//! The background task id a tool result carries.

use serde_json::Value;

use super::safe_id::is_safe_id;

/// The task id Claude Code attaches to a backgrounded call's `toolUseResult`:
/// the string at `backgroundTaskId`, else the one at `background_task_id`,
/// kept only when [`is_safe_id`] accepts it.
pub fn background_task_id(tool_use_result: &Value) -> Option<String> {
    ["backgroundTaskId", "background_task_id"]
        .iter()
        .find_map(|key| tool_use_result.get(*key).and_then(Value::as_str))
        .filter(|id| is_safe_id(id))
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "background_id_tests.rs"]
mod background_id_tests;
