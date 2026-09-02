//! Discovering pending dispute work and parsing the dispute/verdict
//! record files it is read from.
//!
//! Everything here reads `.loom/work/disputes/<stage>/<n>/*.md` on behalf of
//! [`super::AdjudicatorRegistry`]'s polling methods — it never writes.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::dispute::{DisputeRequest, DisputeVerdictRecord};

use super::AdjudicatorRegistry;

/// Discover `(stage_id, dispute_id)` pairs that have `request.md` but
/// no `verdict.md`.
pub(super) fn scan_pending_requests(disputes_root: &Path) -> Result<Vec<(String, u32)>> {
    let mut pending = Vec::new();
    if !disputes_root.exists() {
        return Ok(pending);
    }
    for stage_entry in std::fs::read_dir(disputes_root)? {
        let stage_entry = stage_entry?;
        let path = stage_entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(stage_id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        for inner in std::fs::read_dir(&path)? {
            let inner = inner?;
            let inner_path = inner.path();
            if !inner_path.is_dir() {
                continue;
            }
            let Some(name) = inner_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(dispute_id) = name.parse::<u32>() else {
                continue;
            };
            let req = inner_path.join("request.md");
            let ver = inner_path.join("verdict.md");
            if req.exists() && !ver.exists() {
                pending.push((stage_id.clone(), dispute_id));
            }
        }
    }
    pending.sort();
    Ok(pending)
}

/// Discover `(stage_id, dispute_id)` pairs that have `verdict.md` but
/// no `applied.marker`.
pub(super) fn scan_pending_verdicts(disputes_root: &Path) -> Result<Vec<(String, u32)>> {
    let mut pending = Vec::new();
    if !disputes_root.exists() {
        return Ok(pending);
    }
    for stage_entry in std::fs::read_dir(disputes_root)? {
        let stage_entry = stage_entry?;
        let path = stage_entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(stage_id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        for inner in std::fs::read_dir(&path)? {
            let inner = inner?;
            let inner_path = inner.path();
            if !inner_path.is_dir() {
                continue;
            }
            let Some(name) = inner_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(dispute_id) = name.parse::<u32>() else {
                continue;
            };
            let ver = inner_path.join("verdict.md");
            let applied = inner_path.join("applied.marker");
            if ver.exists() && !applied.exists() {
                pending.push((stage_id.clone(), dispute_id));
            }
        }
    }
    pending.sort();
    Ok(pending)
}

impl AdjudicatorRegistry {
    /// `(stage_id, dispute_id)` pairs with a written verdict that hasn't been
    /// applied yet (no `applied.marker`).
    pub fn pending_verdicts(&self, work_dir: &Path) -> Result<Vec<(String, u32)>> {
        let disputes_root = work_dir.join("disputes");
        if !disputes_root.exists() {
            return Ok(Vec::new());
        }
        scan_pending_verdicts(&disputes_root)
    }

    /// The session id recorded in a dispute's verdict, if the record carries
    /// one. `None` covers both "no verdict written yet" and "a verdict
    /// written before this field existed" — callers that need to tell those
    /// apart already have the verdict file's own existence to check.
    pub fn verdict_session_id(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
    ) -> Option<String> {
        read_verdict_record(&crate::models::dispute::verdict_file(
            &work_dir.join("disputes"),
            stage_id,
            dispute_id,
        ))
        .ok()
        .and_then(|record| record.session_id)
    }

    /// How many disputes on `stage_id` still have no verdict on disk.
    pub fn unanswered_disputes(&self, work_dir: &Path, stage_id: &str) -> Result<usize> {
        Ok(scan_pending_requests(&work_dir.join("disputes"))?
            .into_iter()
            .filter(|(id, _)| id == stage_id)
            .count())
    }

    /// Whether any verdict already recorded for `stage_id` names
    /// `session_id` as the judge that wrote it. A judge that already wrote
    /// its verdict via `loom stage adjudicate` has finished its job: its own
    /// process exiting afterward is an ordinary completion, not a crash —
    /// `orchestrator::monitor::session_events::finished_adjudication_session`
    /// uses this to tell the two apart. `scan` is a private module, so this
    /// method on the already-public `AdjudicatorRegistry` is the path
    /// callers outside `adjudication` reach it through.
    ///
    /// An unreadable or unparseable verdict file is simply not a match; this
    /// never propagates an error and never panics. `false` when the stage
    /// has no dispute directory at all.
    pub fn verdict_written_by(&self, work_dir: &Path, stage_id: &str, session_id: &str) -> bool {
        let disputes_root = work_dir.join("disputes");
        let stage_dir = disputes_root.join(stage_id);
        if !stage_dir.exists() {
            return false;
        }
        dispute_ids(&stage_dir).into_iter().any(|id| {
            read_verdict_record(&crate::models::dispute::verdict_file(
                &disputes_root,
                stage_id,
                id,
            ))
            .ok()
            .and_then(|record| record.session_id)
            .as_deref()
                == Some(session_id)
        })
    }
}

/// The numbered dispute subdirectories under one stage's dispute directory,
/// walked the same way as [`scan_pending_verdicts`].
fn dispute_ids(stage_dir: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(stage_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
        })
        .collect()
}

pub(super) fn read_dispute_request(path: &Path) -> Result<DisputeRequest> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_yaml_frontmatter::<DisputeRequest>(&content)
        .with_context(|| format!("parse dispute request {}", path.display()))
}

pub(super) fn read_verdict_record(path: &Path) -> Result<DisputeVerdictRecord> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_yaml_frontmatter::<DisputeVerdictRecord>(&content)
        .with_context(|| format!("parse verdict record {}", path.display()))
}

/// Pull the YAML frontmatter out of a markdown file and deserialize it.
pub(super) fn parse_yaml_frontmatter<T: serde::de::DeserializeOwned>(content: &str) -> Result<T> {
    let trimmed = content.trim_start();
    let body = trimmed.strip_prefix("---").unwrap_or(trimmed);
    let body = body.trim_start_matches('\n');
    let end = body
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("missing closing '---'"))?;
    let yaml = &body[..end];
    let parsed: T = serde_yaml::from_str(yaml).context("yaml deserialization")?;
    Ok(parsed)
}
