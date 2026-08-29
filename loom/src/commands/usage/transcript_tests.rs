use std::fs;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::json;

use super::*;

fn timestamp(value: &str) -> Result<DateTime<Utc>> {
    Ok(value.parse()?)
}

fn discovered(path: std::path::PathBuf) -> super::super::discovery::DiscoveredFile {
    super::super::discovery::DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "session".to_owned(),
        agent_id: None,
    }
}

fn write_fixture(content: String) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("session.jsonl");
    fs::write(&path, content)?;
    Ok((directory, path))
}

fn assistant(id: Option<&str>, stamp: &str, content: Value, usage: Value) -> String {
    let mut message = json!({"model": "model", "content": content, "usage": usage});
    if let Some(id) = id {
        message["id"] = json!(id);
    }
    json!({"type": "assistant", "timestamp": stamp, "message": message}).to_string()
}

fn parse_content(content: String, since: &str) -> Result<Transcript> {
    let (_directory, path) = write_fixture(content)?;
    // Keep the temporary directory alive while parsing by parsing inside this helper.
    parse(&discovered(path), timestamp(since)?)
}

#[test]
fn duplicate_usage_is_counted_once_and_content_is_merged() -> Result<()> {
    let usage = json!({"input_tokens": 10, "output_tokens": 4});
    let content = format!(
        "{}\n{}\n{}\n",
        assistant(
            Some("same"),
            "2026-08-02T00:00:00Z",
            json!([{"type":"text","text":"one"}]),
            usage
        ),
        assistant(
            Some("same"),
            "2026-08-02T00:00:00Z",
            json!([{"type":"tool_use","id":"t","name":"Bash","input":{}}]),
            json!({"input_tokens": 999})
        ),
        assistant(
            None,
            "2026-08-02T00:01:00Z",
            json!([{"type":"text","text":"two"}]),
            json!({"output_tokens": 3})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    let requests: Vec<_> = transcript.requests().collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].usage.input, 10);
    assert_eq!(requests[0].tool_uses.len(), 1);
    assert_eq!(requests[0].text_chars, 3);
    assert_eq!(transcript.total_usage().output, 7);
    Ok(())
}

#[test]
fn torn_line_and_since_cutoff_are_harmless() -> Result<()> {
    let content = format!(
        "{}\n{{\"type\":\"assistant\"\n{}\n",
        assistant(
            None,
            "2026-07-01T00:00:00Z",
            json!([]),
            json!({"output_tokens": 1})
        ),
        assistant(
            None,
            "2026-08-02T00:00:00Z",
            json!([]),
            json!({"output_tokens": 2})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    assert_eq!(transcript.requests().count(), 1);
    assert_eq!(transcript.total_usage().output, 2);
    Ok(())
}

#[test]
fn cache_creation_uses_nested_split_or_five_minute_default() -> Result<()> {
    let content = format!(
        "{}\n{}\n",
        assistant(
            None,
            "2026-08-02T00:00:00Z",
            json!([]),
            json!({"cache_creation_input_tokens": 10, "cache_creation": {"ephemeral_5m_input_tokens": 3, "ephemeral_1h_input_tokens": 7}})
        ),
        assistant(
            None,
            "2026-08-02T00:01:00Z",
            json!([]),
            json!({"cache_creation_input_tokens": 8})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    let requests: Vec<_> = transcript.requests().collect();
    assert_eq!(
        (
            requests[0].usage.ephemeral_5m,
            requests[0].usage.ephemeral_1h
        ),
        (3, 7)
    );
    assert_eq!(
        (
            requests[1].usage.ephemeral_5m,
            requests[1].usage.ephemeral_1h
        ),
        (8, 0)
    );
    Ok(())
}

#[test]
fn ephemeral_split_always_reconciles_to_cache_creation() -> Result<()> {
    // S2's cache-write term is priced entirely off ephemeral_5m/ephemeral_1h,
    // so every shape of the raw `usage` object must resolve to the same
    // invariant: ephemeral_5m + ephemeral_1h == cache_creation.
    let content = format!(
        "{}\n{}\n{}\n",
        // Flat field and nested split agree.
        assistant(
            None,
            "2026-08-02T00:00:00Z",
            json!([]),
            json!({"cache_creation_input_tokens": 10, "cache_creation": {"ephemeral_5m_input_tokens": 3, "ephemeral_1h_input_tokens": 7}})
        ),
        // Nested split present, flat field missing (0): trust the split.
        assistant(
            None,
            "2026-08-02T00:01:00Z",
            json!([]),
            json!({"cache_creation": {"ephemeral_5m_input_tokens": 4, "ephemeral_1h_input_tokens": 6}})
        ),
        // Nested object present but with unrecognised keys: both ephemerals
        // read 0 while the flat field carries the real total.
        assistant(
            None,
            "2026-08-02T00:02:00Z",
            json!([]),
            json!({"cache_creation_input_tokens": 12, "cache_creation": {"unexpected_key": 99}})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    let requests: Vec<_> = transcript.requests().collect();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(
            request.usage.ephemeral_5m + request.usage.ephemeral_1h,
            request.usage.cache_creation
        );
    }
    assert_eq!(requests[0].usage.cache_creation, 10);
    assert_eq!(
        (
            requests[1].usage.cache_creation,
            requests[1].usage.ephemeral_5m,
            requests[1].usage.ephemeral_1h
        ),
        (10, 4, 6)
    );
    assert_eq!(
        (
            requests[2].usage.cache_creation,
            requests[2].usage.ephemeral_5m,
            requests[2].usage.ephemeral_1h
        ),
        (12, 12, 0)
    );
    Ok(())
}

#[test]
fn duplicate_line_zero_usage_defers_to_a_later_nonzero_duplicate() -> Result<()> {
    let content = format!(
        "{}\n{}\n",
        assistant(
            Some("same"),
            "2026-08-02T00:00:00Z",
            json!([{"type":"text","text":"one"}]),
            json!({})
        ),
        assistant(
            Some("same"),
            "2026-08-02T00:00:00Z",
            json!([]),
            json!({"input_tokens": 10, "output_tokens": 4})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    let requests: Vec<_> = transcript.requests().collect();
    assert_eq!(requests.len(), 1);
    assert_eq!((requests[0].usage.input, requests[0].usage.output), (10, 4));
    Ok(())
}

#[test]
fn first_user_entry_survives_the_since_cutoff() -> Result<()> {
    let content = format!(
        "{}\n{}\n",
        json!({"type":"user", "timestamp":"2026-07-01T00:00:00Z", "message":{"content":"preamble"}}),
        assistant(
            None,
            "2026-08-02T00:00:00Z",
            json!([]),
            json!({"output_tokens": 1})
        ),
    );
    let transcript = parse_content(content, "2026-08-01T00:00:00Z")?;
    let first = transcript
        .first_user_entry
        .as_ref()
        .expect("first user entry survives the cutoff");
    assert_eq!(first.text, "preamble");
    assert!(!transcript
        .entries
        .iter()
        .any(|entry| matches!(entry, Entry::User(user) if user.text == "preamble")));
    Ok(())
}

#[test]
fn user_tool_results_extract_string_and_text_array() -> Result<()> {
    let content = format!(
        "{}\n{}\n",
        json!({"type":"user", "timestamp":"2026-08-02T00:00:00Z", "message":{"content":[{"type":"tool_result", "tool_use_id":"one", "content":"plain"}]}}),
        json!({"type":"user", "timestamp":"2026-08-02T00:01:00Z", "message":{"content":[{"type":"tool_result", "tool_use_id":"two", "content":[{"type":"text", "text":"a"}, {"type":"text", "text":"b"}]}]}}),
    );
    let entries = parse_content(content, "2026-08-01T00:00:00Z")?.entries;
    let text: Vec<_> = entries
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::User(user) => Some((user.tool_use_id, user.text)),
            _ => None,
        })
        .collect();
    assert_eq!(
        text,
        vec![
            (Some("one".to_owned()), "plain".to_owned()),
            (Some("two".to_owned()), "ab".to_owned())
        ]
    );
    Ok(())
}
