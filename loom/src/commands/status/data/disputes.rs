//! The disputes already filed against a blocked stage, read from
//! `.loom/work/disputes/<stage>/<n>/{request.md,verdict.md}` for the attention
//! card.
//!
//! The record formats are written by the dispute pipeline and change with it,
//! so every lookup here is lenient: a missing key or an unreadable file leaves
//! that part of the line out, never fails the status payload.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus};
use crate::parser::frontmatter::extract_yaml_frontmatter;

use super::sanitize::valid_stage_id;

/// Most recent disputes one stage contributes to a payload the whole dashboard
/// shares. Revisit if operators need the older rulings on the card.
const MAX_DISPUTES: usize = 5;

/// Longest dispute record read; a record carries evidence and citations, and a
/// larger file is skipped rather than loaded every second.
const MAX_RECORD_BYTES: u64 = 1024 * 1024;

/// One acceptance-criteria edit a dispute's verdict applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputeOp {
    /// `replace`, `delete`, `append`, ... as the verdict names it.
    pub op: String,
    /// The criterion the edit targeted, counted from 0.
    pub index: Option<u64>,
}

/// One dispute of a blocked stage, as the card summarizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputeLine {
    pub id: u32,
    /// What the dispute contested: `criterion`, `integrity`, ...
    pub kind: String,
    /// The ruling (`accept`, `reject`, ...); `None` while no verdict is filed.
    pub verdict: Option<String>,
    /// The plan field the verdict patched.
    pub field: Option<String>,
    /// The edits the verdict made to that field.
    pub ops: Vec<DisputeOp>,
}

/// The newest [`MAX_DISPUTES`] disputes of a blocked stage, oldest first. Every
/// other stage carries none: the card that shows them exists only while the
/// stage is blocked.
pub(super) fn blocked_disputes(stage: &Stage, work_dir: &WorkDir) -> Vec<DisputeLine> {
    if stage.status != StageStatus::Blocked || !valid_stage_id(&stage.id) {
        return Vec::new();
    }
    read_disputes(&work_dir.disputes_dir().join(&stage.id))
}

fn read_disputes(stage_dir: &Path) -> Vec<DisputeLine> {
    let Ok(entries) = fs::read_dir(stage_dir) else {
        return Vec::new();
    };
    let mut ids: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect();
    ids.sort_unstable();
    let newest = ids.len().saturating_sub(MAX_DISPUTES);
    ids[newest..]
        .iter()
        .map(|&id| read_dispute(&stage_dir.join(id.to_string()), id))
        .collect()
}

fn read_dispute(dir: &Path, id: u32) -> DisputeLine {
    let request = frontmatter(&dir.join("request.md"));
    let verdict = frontmatter(&dir.join("verdict.md"));
    let ruling = verdict.get("verdict");
    let patch = ruling.and_then(|ruling| ruling.get("plan_patch"));
    DisputeLine {
        id,
        kind: text(request.get("kind")).unwrap_or_else(|| "dispute".to_owned()),
        verdict: text(ruling.and_then(|ruling| ruling.get("verdict"))),
        field: text(patch.and_then(|patch| patch.get("field"))),
        ops: patch.map(patch_ops).unwrap_or_default(),
    }
}

/// The canonical form is a `patches` list; a single `patch` object is read too.
fn patch_ops(plan_patch: &Value) -> Vec<DisputeOp> {
    let listed = plan_patch.get("patches").and_then(Value::as_array);
    let single = plan_patch.get("patch").filter(|patch| patch.is_object());
    listed
        .into_iter()
        .flatten()
        .chain(single)
        .filter_map(|patch| {
            Some(DisputeOp {
                op: text(patch.get("op"))?,
                index: patch.get("index").and_then(Value::as_u64),
            })
        })
        .collect()
}

fn text(value: Option<&Value>) -> Option<String> {
    value?.as_str().map(str::to_owned)
}

/// The record's frontmatter, or `Null` (every lookup on it misses) when the
/// file is absent, oversized or malformed.
fn frontmatter(path: &Path) -> Value {
    let small = fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_RECORD_BYTES);
    if !small {
        return Value::Null;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|content| extract_yaml_frontmatter(&content).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
#[path = "disputes_tests.rs"]
mod tests;
