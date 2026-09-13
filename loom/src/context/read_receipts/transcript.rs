use super::{ContentClass, Payload, MAX_RECEIPT_BYTES};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const MAX_TRANSCRIPT_TAIL_BYTES: usize = 128 * 1024;

pub(super) struct CorrelatedResult {
    pub class: ContentClass,
    pub bytes: Vec<u8>,
    pub tool_use_id: String,
}

pub(super) fn correlated_result(payload: &Payload) -> Option<CorrelatedResult> {
    let path = payload.transcript_path.as_deref()?;
    let input = read_tail_window(path)?;
    let rows = input
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Option<Vec<_>>>()?;
    let ids = rows
        .iter()
        .flat_map(|row| matching_ids(row, payload))
        .collect::<Vec<_>>();
    if ids.len() != 1 {
        return None;
    }
    if result_count(&rows, &ids[0]) != 1 {
        return None;
    }
    let mut candidates = rows
        .windows(2)
        .filter_map(|pair| candidate(&pair[0], &pair[1], payload, &ids[0]))
        .collect::<Vec<_>>();
    (candidates.len() == 1).then(|| candidates.pop()).flatten()
}

fn read_tail_window(path: &Path) -> Option<String> {
    crate::models::forward_receipt::locator::read_bounded_prefix(path, 0).ok()?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    metadata.file_type().is_file().then_some(())?;
    let length = metadata.len();
    file.seek(SeekFrom::Start(
        length.saturating_sub(MAX_TRANSCRIPT_TAIL_BYTES as u64),
    ))
    .ok()?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(length.min(MAX_TRANSCRIPT_TAIL_BYTES as u64)).ok()?);
    file.take(MAX_TRANSCRIPT_TAIL_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let bytes = if length > MAX_TRANSCRIPT_TAIL_BYTES as u64 {
        &bytes[bytes.iter().position(|byte| *byte == b'\n')? + 1..]
    } else {
        bytes.as_slice()
    };
    String::from_utf8(bytes.to_vec()).ok()
}

fn matching_ids(row: &Value, payload: &Payload) -> Vec<String> {
    if row.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    row.pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| matches_use(block, payload))
        .filter_map(|block| block.get("id").and_then(Value::as_str))
        .filter(|id| {
            payload
                .tool_use_id
                .as_ref()
                .is_none_or(|expected| expected == id)
        })
        .map(str::to_owned)
        .collect()
}

fn result_count(rows: &[Value], id: &str) -> usize {
    rows.iter()
        .filter(|row| row.get("type").and_then(Value::as_str) == Some("user"))
        .filter_map(|row| row.pointer("/message/content").and_then(Value::as_array))
        .flatten()
        .filter(|block| {
            block.get("type").and_then(Value::as_str) == Some("tool_result")
                && block.get("tool_use_id").and_then(Value::as_str) == Some(id)
        })
        .count()
}

fn candidate(
    assistant: &Value,
    user: &Value,
    payload: &Payload,
    id: &str,
) -> Option<CorrelatedResult> {
    (assistant.get("type")?.as_str()? == "assistant" && user.get("type")?.as_str()? == "user")
        .then_some(())?;
    let uses = assistant.pointer("/message/content")?.as_array()?;
    let results = user.pointer("/message/content")?.as_array()?;
    let matching = uses
        .iter()
        .filter(|block| {
            matches_use(block, payload) && block.get("id").and_then(Value::as_str) == Some(id)
        })
        .count();
    (matching == 1).then_some(())?;
    let results = results
        .iter()
        .filter(|block| {
            block.get("type").and_then(Value::as_str) == Some("tool_result")
                && block.get("tool_use_id").and_then(Value::as_str) == Some(id)
        })
        .collect::<Vec<_>>();
    (results.len() == 1).then_some(())?;
    result_body(results[0]).map(|(class, bytes)| CorrelatedResult {
        class,
        bytes,
        tool_use_id: id.to_owned(),
    })
}

fn matches_use(block: &Value, payload: &Payload) -> bool {
    block.get("type").and_then(Value::as_str) == Some("tool_use")
        && block.get("name").and_then(Value::as_str) == Some("Read")
        && block.get("input") == Some(&payload.input)
        && block
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(super::is_safe_id)
}

fn result_body(block: &Value) -> Option<(ContentClass, Vec<u8>)> {
    (!block.get("is_error")?.as_bool()?).then_some(())?;
    match block.get("content")? {
        Value::String(text) => bounded(ContentClass::Text, text.as_bytes().to_vec()),
        Value::Array(blocks) => array_body(blocks),
        _ => None,
    }
}

fn array_body(blocks: &[Value]) -> Option<(ContentClass, Vec<u8>)> {
    (!blocks.is_empty()).then_some(())?;
    let media = blocks.iter().any(|block| {
        matches!(
            block.get("type").and_then(Value::as_str),
            Some("image" | "document")
        )
    });
    let known = blocks.iter().all(|block| {
        matches!(
            block.get("type").and_then(Value::as_str),
            Some("text" | "image" | "document")
        )
    });
    known.then_some(())?;
    if media {
        return bounded(ContentClass::Media, serde_json::to_vec(blocks).ok()?);
    }
    let text = blocks
        .iter()
        .map(|block| block.get("text")?.as_str())
        .collect::<Option<Vec<_>>>()?
        .concat();
    bounded(ContentClass::Text, text.into_bytes())
}

fn bounded(class: ContentClass, bytes: Vec<u8>) -> Option<(ContentClass, Vec<u8>)> {
    (bytes.len() <= MAX_RECEIPT_BYTES).then_some((class, bytes))
}
