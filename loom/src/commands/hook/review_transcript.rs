//! Reads a `loom-code-reviewer` subagent's report from its transcript tail,
//! for `review_harvest`.
//!
//! A reviewer's report may arrive two ways: as the text of its final
//! assistant turn, or handed back through a `SubagentHandback` tool call
//! whose `message` input holds the report. The hand-back is preferred; if
//! both parse and disagree, the hand-back wins and the disagreement is
//! returned alongside the parsed review.

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};

use anyhow::{ensure, Context, Result};
use serde_json::Value;

use crate::commands::hook::review_harvest::HarvestInput;
use crate::commands::subagents::{is_assistant, text_blocks};
use crate::models::forward_receipt::transcript::TranscriptIdentity;
use crate::subagent_lifecycle::reject_symlink_components;
use crate::verify::review::report::{parse_review, ParsedReview};

/// The final message sits at the end of the transcript; only this much of the
/// tail is read.
const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// The reviewer's parsed review from `input`'s transcript, and, when both the
/// hand-back and final text parse but disagree, why the hand-back was kept.
pub(super) fn reviewer_review(
    input: &HarvestInput,
) -> Result<(Result<ParsedReview, String>, Option<String>)> {
    let (handback, final_text) = reviewer_report(input)?;
    Ok(select_review(handback, final_text))
}

/// The reviewer's report candidates, once the transcript is proven to be the
/// stop event's own: an absolute, normalized, symlink-free regular file at
/// `<project>/<session_id>/subagents/agent-<agent_id>.jsonl`.
fn reviewer_report(input: &HarvestInput) -> Result<(Option<String>, Option<String>)> {
    let path = input.transcript_path.as_path();
    ensure_plain_path(path)?;
    let identity = TranscriptIdentity::from_path(path)?;
    ensure!(
        path.extension() == Some(OsStr::new("jsonl"))
            && identity.parent_session_id == input.session_id
            && identity.agent_id == input.agent_id,
        "the transcript path does not belong to the stop event's session and agent"
    );
    Ok(review_candidates(&read_tail(path)?))
}

/// The parsed review, preferring the hand-back candidate's block over the
/// final assistant text's. If both parse and disagree, the hand-back wins and
/// the second return value carries why.
fn select_review(
    handback: Option<String>,
    final_text: Option<String>,
) -> (Result<ParsedReview, String>, Option<String>) {
    let handback = handback.as_deref().map(parse_review);
    let final_text = final_text.as_deref().map(parse_review);
    match (handback, final_text) {
        (Some(Ok(from_handback)), Some(Ok(from_final))) => {
            let discrepancy = (from_final != from_handback).then(|| {
                "the hand-back report and the final assistant text both parse but disagree; \
                 using the hand-back"
                    .to_string()
            });
            (Ok(from_handback), discrepancy)
        }
        (Some(Ok(from_handback)), _) => (Ok(from_handback), None),
        (_, Some(Ok(from_final))) => (Ok(from_final), None),
        (Some(Err(reason)), _) => (Err(reason), None),
        (None, Some(Err(reason))) => (Err(reason), None),
        (None, None) => (
            Err(
                "the reviewer's transcript has no final assistant text or hand-back report"
                    .to_string(),
            ),
            None,
        ),
    }
}

fn ensure_plain_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_))),
        "the transcript path is not absolute and normalized"
    );
    reject_symlink_components(path).context("the transcript path is not symlink-free")
}

/// The complete rows in the last `TRANSCRIPT_TAIL_BYTES` of `path`.
fn read_tail(path: &Path) -> Result<String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let metadata = file
        .metadata()
        .context("reading the transcript's metadata")?;
    ensure!(metadata.is_file(), "the transcript is not a regular file");
    let start = metadata.len().saturating_sub(TRANSCRIPT_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .context("seeking to the transcript's tail")?;
    let mut bytes = Vec::new();
    file.take(TRANSCRIPT_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .context("reading the transcript")?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail that starts mid-row drops that partial row.
    let rows = match start {
        0 => &text[..],
        _ => text.split_once('\n').map_or("", |(_, rest)| rest),
    };
    Ok(rows.to_owned())
}

/// The reviewer's report candidates in `transcript`: the `message` input of
/// the last `SubagentHandback` tool call, and the text blocks of the last
/// assistant entry that has any, joined. Unparseable rows are skipped, so a
/// tail that starts mid-row loses only that row. Only assistant entries are
/// read for either candidate, so a `loom-review` block quoted back to the
/// reviewer inside a tool result cannot be mistaken for its report.
fn review_candidates(transcript: &str) -> (Option<String>, Option<String>) {
    let assistant_rows: Vec<Value> = transcript
        .lines()
        .filter_map(|row| serde_json::from_str::<Value>(row).ok())
        .filter(is_assistant)
        .collect();
    let handback = assistant_rows.iter().rev().find_map(handback_message);
    let final_text = assistant_rows
        .iter()
        .rev()
        .map(|entry| text_blocks(entry).join("\n"))
        .find(|text| !text.is_empty());
    (handback, final_text)
}

/// The `message` input of the last `SubagentHandback` tool_use block in
/// `entry`, if any.
fn handback_message(entry: &Value) -> Option<String> {
    entry
        .get("message")?
        .get("content")?
        .as_array()?
        .iter()
        .rev()
        .find(|block| is_handback_block(block))
        .and_then(|block| block.pointer("/input/message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn is_handback_block(block: &Value) -> bool {
    block.get("type").and_then(Value::as_str) == Some("tool_use")
        && block.get("name").and_then(Value::as_str) == Some("SubagentHandback")
}
