//! Reading the records a probe names out of transcript and spool lines.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The text lines of `path`; none when it cannot be read.
pub fn lines_of(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines().map(str::to_owned).collect()
}

/// Each line parsed as JSON; an unparsed line becomes `Null`, so positions are kept.
pub fn parsed(lines: &[String]) -> Vec<Value> {
    let parse = |line: &String| serde_json::from_str(line).unwrap_or(Value::Null);
    lines.iter().map(parse).collect()
}

/// The `tool_use` blocks of assistant records named `tool` that satisfy `keep`.
pub fn tool_uses(records: &[Value], tool: &str, keep: impl Fn(&Value) -> bool) -> Vec<Value> {
    let blocks = records
        .iter()
        .filter(|record| record["type"] == "assistant")
        .filter_map(|record| record.pointer("/message/content")?.as_array())
        .flatten();
    let named = blocks.filter(|block| block["type"] == "tool_use" && block["name"] == tool);
    named
        .filter(|block| keep(&block["input"]))
        .cloned()
        .collect()
}

/// Whether a tool input asks to run in the background. The model sometimes sends the flag
/// as the string `"true"`, and Claude Code 2.1.321 still launches the task in the background.
pub fn backgrounded(input: &Value) -> bool {
    let flag = &input["run_in_background"];
    *flag == Value::Bool(true) || flag.as_str() == Some("true")
}

/// The whole user record that carries the `tool_result` of `tool_use_id`. Only a `user`
/// record counts: an `api-request-blob` record repeats the earlier results in its request.
pub fn tool_result<'a>(records: &'a [Value], tool_use_id: &str) -> Option<&'a Value> {
    let users = records.iter().filter(|record| record["type"] == "user");
    users.into_iter().find(|record| {
        let content = record.pointer("/message/content").and_then(Value::as_array);
        let blocks = content.map(|blocks| blocks.iter()).into_iter().flatten();
        let mut results = blocks.filter(|block| block["type"] == "tool_result");
        results.any(|block| block["tool_use_id"] == tool_use_id)
    })
}

/// The lines containing `needle`.
pub fn containing<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
    lines.iter().filter(|line| line.contains(needle)).collect()
}

/// The spool entries of hook `event`.
pub fn spooled<'a>(spool: &'a [Value], event: &str) -> Vec<&'a Value> {
    spool.iter().filter(|line| line["event"] == event).collect()
}

/// The `stop_reason` of the last assistant record, `"(no assistant record)"` when none.
pub fn last_stop_reason(records: &[Value]) -> String {
    let last = records.iter().rev().find(|r| r["type"] == "assistant");
    match last {
        Some(record) => record
            .pointer("/message/stop_reason")
            .map_or_else(|| "(no stop_reason field)".to_owned(), Value::to_string),
        None => "(no assistant record)".to_owned(),
    }
}

/// `<transcript dir>/<session id>/subagents/agent-<id>.jsonl` for each agent, sorted, with
/// the agent id from the file name.
pub fn subagent_files(main: &Path) -> Vec<(String, PathBuf)> {
    let (Some(parent), Some(stem)) = (main.parent(), main.file_stem()) else {
        return Vec::new();
    };
    let dir = parent.join(stem).join("subagents");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
            Some((id.to_owned(), entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// The `.meta.json` beside an agent's `.jsonl`.
pub fn meta_of(jsonl: &Path) -> PathBuf {
    jsonl.with_extension("meta.json")
}

/// The values joined one per line, for a note.
pub fn joined<T: std::fmt::Display>(values: impl IntoIterator<Item = T>) -> String {
    let lines: Vec<String> = values.into_iter().map(|v| v.to_string()).collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_uses_and_results_are_found_by_id() {
        let records = vec![
            json!({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "t1", "name": "Agent",
                 "input": {"run_in_background": true}},
                {"type": "tool_use", "id": "t2", "name": "Agent", "input": {}}
            ], "stop_reason": "tool_use"}}),
            json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "ok"}
            ]}}),
        ];
        let background = tool_uses(&records, "Agent", backgrounded);
        assert_eq!(background.len(), 1);
        assert_eq!(background[0]["id"], "t1");
        assert!(tool_result(&records, "t1").is_none());
        assert_eq!(tool_result(&records, "t2"), Some(&records[1]));
        assert_eq!(last_stop_reason(&records), "\"tool_use\"");
    }

    #[test]
    fn a_request_blob_repeating_a_result_is_not_the_result_record() {
        let result = json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}
        ]}});
        let blob = json!({"type": "api-request-blob", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}
        ]}});
        let records = vec![blob, result.clone()];
        assert_eq!(tool_result(&records, "t1"), Some(&result));
    }
}
