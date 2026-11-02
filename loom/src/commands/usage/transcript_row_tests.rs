//! Which rows the scan keeps and which ones cost a transcript its provable start.

use anyhow::Result;
use serde_json::json;

use super::super::discovery::DiscoveredFile;
use super::super::time_range::TimeRange;
use super::*;

const SINCE: &str = "2026-08-01T00:00:00Z";

fn assistant(id: &str, stamp: &str) -> String {
    json!({
        "type": "assistant",
        "timestamp": stamp,
        "message": {"id": id, "model": "model", "content": [], "usage": {"output_tokens": 1}}
    })
    .to_string()
}

fn parse_rows(rows: &[String]) -> Result<Transcript> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("session.jsonl");
    std::fs::write(&path, format!("{}\n", rows.join("\n")))?;
    let file = DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: "session".to_owned(),
        agent_id: None,
    };
    let range = TimeRange {
        since: SINCE.parse()?,
        until: None,
    };
    parse(&file, &range)
}

fn start_of(transcript: &Transcript) -> Option<bool> {
    transcript
        .requests()
        .next()
        .expect("a request")
        .normalization
        .true_fresh_start
}

#[test]
fn a_bookkeeping_row_with_no_timestamp_leaves_the_start_provable() -> Result<()> {
    let bookkeeping = json!({"type": "last-prompt", "lastPrompt": "go"}).to_string();
    let first = assistant("m1", "2026-08-02T00:00:00Z");
    for rows in [
        [bookkeeping.clone(), first.clone()],
        [first.clone(), bookkeeping.clone()],
    ] {
        let transcript = parse_rows(&rows)?;
        assert_eq!(start_of(&transcript), Some(true));
        assert_eq!(transcript.diagnostics.missing_timestamp_rows, 1);
    }
    Ok(())
}

#[test]
fn a_malformed_final_line_leaves_the_start_provable() -> Result<()> {
    let rows = [
        assistant("m1", "2026-08-02T00:00:00Z"),
        r#"{"type":"assist"#.to_owned(),
    ];
    let transcript = parse_rows(&rows)?;
    assert_eq!(start_of(&transcript), Some(true));
    assert_eq!(transcript.diagnostics.malformed_rows, 1);
    Ok(())
}

#[test]
fn an_unplaced_row_before_the_first_request_makes_the_start_unknown() -> Result<()> {
    let first = assistant("m1", "2026-08-02T00:00:00Z");
    let unplaced = [
        json!({"type": "user", "message": {"content": "hi"}}).to_string(),
        json!({"type": "assistant", "message": {"content": []}}).to_string(),
        json!({"type": "attachment", "attachment": {"type": "x"}}).to_string(),
    ];
    for row in unplaced {
        let transcript = parse_rows(&[row, first.clone()])?;
        assert_eq!(start_of(&transcript), None);
        assert_eq!(transcript.diagnostics.missing_timestamp_rows, 1);
    }
    let torn = parse_rows(&["{not json".to_owned(), first])?;
    assert_eq!(start_of(&torn), None);
    assert_eq!(torn.diagnostics.malformed_rows, 1);
    Ok(())
}

#[test]
fn an_unplaced_row_after_the_first_request_is_only_counted() -> Result<()> {
    let rows = [
        assistant("m1", "2026-08-02T00:00:00Z"),
        json!({"type": "user", "message": {"content": "hi"}}).to_string(),
        assistant("m2", "2026-08-02T00:01:00Z"),
    ];
    let transcript = parse_rows(&rows)?;
    assert_eq!(start_of(&transcript), Some(true));
    assert_eq!(transcript.diagnostics.missing_timestamp_rows, 1);
    Ok(())
}

#[test]
fn an_unplaced_row_after_only_a_synthetic_request_makes_the_start_unknown() -> Result<()> {
    let synthetic = json!({
        "type": "assistant",
        "timestamp": "2026-08-02T00:00:00Z",
        "message": {"id": "e1", "model": SYNTHETIC_MODEL, "content": []}
    })
    .to_string();
    let rows = [
        synthetic,
        "{not json".to_owned(),
        assistant("m1", "2026-08-02T00:01:00Z"),
    ];
    let transcript = parse_rows(&rows)?;
    let first = transcript
        .requests()
        .find(|request| request.model != SYNTHETIC_MODEL)
        .expect("a real request");
    assert_eq!(first.normalization.true_fresh_start, None);
    Ok(())
}

fn user_row(uuid: Option<&str>, stamp: &str, text: &str) -> String {
    let mut row = json!({"type": "user", "timestamp": stamp, "message": {"content": text}});
    if let Some(uuid) = uuid {
        row["uuid"] = json!(uuid);
    }
    row.to_string()
}

fn attachment_row(uuid: Option<&str>, stamp: &str) -> String {
    let mut row = json!({
        "type": "attachment",
        "timestamp": stamp,
        "attachment": {"type": "hook_additional_context"},
        "rendered": [{"content": "abcd"}]
    });
    if let Some(uuid) = uuid {
        row["uuid"] = json!(uuid);
    }
    row.to_string()
}

fn user_texts(transcript: &Transcript) -> Vec<&str> {
    transcript
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::User(user) => Some(user.text.as_str()),
            Entry::Assistant(_) => None,
        })
        .collect()
}

#[test]
fn a_repeated_uuid_drops_the_user_or_attachment_copy() -> Result<()> {
    let stamp = "2026-08-02T00:00:30Z";
    let rows = [
        user_row(Some("u1"), stamp, "first"),
        assistant("m1", "2026-08-02T00:01:00Z"),
        user_row(Some("u1"), stamp, "copy"),
        user_row(Some("u2"), stamp, "second"),
        attachment_row(Some("a1"), stamp),
        attachment_row(Some("a1"), stamp),
    ];
    let transcript = parse_rows(&rows)?;
    assert_eq!(user_texts(&transcript), ["first", "second"]);
    assert_eq!(transcript.attachments.len(), 1);
    Ok(())
}

#[test]
fn a_row_with_no_uuid_is_never_dropped() -> Result<()> {
    let stamp = "2026-08-02T00:00:30Z";
    let rows = [
        user_row(None, stamp, "same"),
        user_row(None, stamp, "same"),
        user_row(Some(""), stamp, "same"),
        user_row(Some(""), stamp, "same"),
        attachment_row(None, stamp),
        attachment_row(None, stamp),
    ];
    let transcript = parse_rows(&rows)?;
    assert_eq!(user_texts(&transcript).len(), 4);
    assert_eq!(transcript.attachments.len(), 2);
    Ok(())
}

#[test]
fn the_first_user_entry_ignores_the_uuid_set() -> Result<()> {
    let rows = [
        user_row(Some("u1"), "2026-07-01T00:00:00Z", "preamble"),
        user_row(Some("u1"), "2026-08-02T00:00:00Z", "copy"),
    ];
    let transcript = parse_rows(&rows)?;
    let first = transcript.first_user_entry.as_ref().expect("first entry");
    assert_eq!(first.text, "preamble");
    assert!(user_texts(&transcript).is_empty());
    Ok(())
}

#[test]
fn an_attachment_counts_only_real_requests_before_it() -> Result<()> {
    let synthetic = json!({
        "type": "assistant",
        "timestamp": "2026-08-02T00:00:30Z",
        "message": {"id": "e1", "model": SYNTHETIC_MODEL, "content": []}
    })
    .to_string();
    let rows = [
        assistant("m1", "2026-08-02T00:00:00Z"),
        synthetic,
        attachment_row(None, "2026-08-02T00:01:00Z"),
    ];
    let transcript = parse_rows(&rows)?;
    assert_eq!(transcript.attachments[0].requests_before, 1);
    Ok(())
}
