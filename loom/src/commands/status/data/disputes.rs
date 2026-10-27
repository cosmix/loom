//! Readers for the dispute records under
//! `.loom/work/disputes/<stage>/<n>/{request.md,verdict.md}`, used by the
//! stage's ended rounds (`rounds.rs`).
//!
//! The record formats are written by the dispute pipeline and change with it,
//! so every lookup here is lenient: a missing key or an unreadable file leaves
//! that part out, never fails the status payload.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::orchestrator::adjudication::dispute_ids;
use crate::parser::frontmatter::extract_yaml_frontmatter;

/// Longest dispute record read; a record carries evidence and citations, and a
/// larger file is skipped rather than loaded every second.
const MAX_RECORD_BYTES: u64 = 1024 * 1024;

pub(super) fn sorted_dispute_ids(stage_dir: &Path) -> Vec<u32> {
    let mut ids = dispute_ids(stage_dir);
    ids.sort_unstable();
    ids
}

pub(super) fn timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

pub(super) fn text(value: Option<&Value>) -> Option<String> {
    value?.as_str().map(str::to_owned)
}

/// The record's frontmatter, or `Null` (every lookup on it misses) when the
/// file is absent, oversized or malformed.
pub(super) fn frontmatter(path: &Path) -> Value {
    let small = fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_RECORD_BYTES);
    if !small {
        return Value::Null;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| extract_yaml_frontmatter(&content).ok())
        .unwrap_or(Value::Null)
}
