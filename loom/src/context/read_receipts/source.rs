use super::{digest, Identity, MAX_RECEIPT_BYTES};
use serde_json::Value;
use std::path::{Path, PathBuf};

const MAX_SOURCE_SCAN_BYTES: usize = MAX_RECEIPT_BYTES * 2;
const MAX_RANGE_LINES: u64 = 4_096;

pub(super) fn normalized_path(cwd: &Path, raw: &str) -> Option<String> {
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    Some(path.canonicalize().ok()?.to_string_lossy().into_owned())
}

pub(super) fn range(input: &Value) -> Option<String> {
    let pages = match input.get("pages") {
        Some(value) => Some(normalized_pages(value)?),
        None => None,
    };
    let offset = match input.get("offset") {
        Some(value) => Some(value.as_u64()?),
        None => None,
    };
    let limit = match input.get("limit") {
        Some(value) => Some(value.as_u64()?),
        None => None,
    };
    match (offset, limit) {
        (None, None) => pages.map_or_else(|| Some("full".to_owned()), Some),
        (offset, limit) => Some(format!(
            "{}:{}",
            offset.unwrap_or(0),
            limit.map_or_else(|| "*".to_owned(), |value| value.to_string())
        )),
    }
}

fn normalized_pages(value: &Value) -> Option<String> {
    let pages = match value {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.as_u64()?.to_string(),
        _ => return None,
    };
    (!pages.is_empty() && pages.len() <= 128).then(|| format!("pages:{pages}"))
}

pub(super) fn generation(identity: &Identity, epoch: &str) -> Option<String> {
    let bytes = selected_bytes(Path::new(&identity.path), &identity.range)?;
    let capacity = bytes.len() + identity.range.len() + epoch.len() + 2;
    let mut input = Vec::with_capacity(capacity);
    input.extend_from_slice(&bytes);
    input.push(0);
    input.extend_from_slice(identity.range.as_bytes());
    input.push(0);
    input.extend_from_slice(epoch.as_bytes());
    Some(digest(&input))
}

fn selected_bytes(path: &Path, range: &str) -> Option<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    metadata.file_type().is_file().then_some(())?;
    if range == "full" || range.starts_with("pages:") {
        let input = read_prefix(path, MAX_RECEIPT_BYTES + 1)?;
        let is_text = std::str::from_utf8(&input).is_ok();
        return (metadata.len() <= MAX_RECEIPT_BYTES as u64
            && input.len() <= MAX_RECEIPT_BYTES
            && is_text)
            .then_some(input);
    }
    let (offset, limit) = parse_range(range)?;
    if offset > MAX_RANGE_LINES {
        return None;
    }
    let input = read_prefix(path, MAX_SOURCE_SCAN_BYTES + 1)?;
    let is_bounded_text =
        input.len() <= MAX_SOURCE_SCAN_BYTES && std::str::from_utf8(&input).is_ok();
    is_bounded_text.then_some(())?;
    let complete = metadata.len() <= u64::try_from(input.len()).ok()?;
    select_lines(&input, offset, limit, complete)
}

fn read_prefix(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use crate::models::forward_receipt::locator::read_bounded_prefix;
    let text = read_bounded_prefix(path, limit).ok()?;
    Some(text.into_bytes())
}

fn parse_range(range: &str) -> Option<(u64, Option<u64>)> {
    let (offset, limit) = range.split_once(':')?;
    let limit = if limit == "*" {
        None
    } else {
        Some(limit.parse().ok()?)
    };
    Some((offset.parse().ok()?, limit))
}

fn select_lines(input: &[u8], offset: u64, limit: Option<u64>, complete: bool) -> Option<Vec<u8>> {
    let start = usize::try_from(offset).ok()?;
    let count = match limit {
        Some(value) => usize::try_from(value).ok()?,
        None => usize::MAX,
    };
    let end = start.checked_add(count)?;
    let mut selected = Vec::new();
    for (index, line) in input.split_inclusive(|byte| *byte == b'\n').enumerate() {
        if index >= start && index < end {
            selected.extend_from_slice(line);
        }
    }
    let is_valid = !selected.is_empty()
        && selected.len() <= MAX_RECEIPT_BYTES
        && (limit.is_some() || complete);
    is_valid.then_some(selected)
}
