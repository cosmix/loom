//! Discovering pending dispute work and parsing the dispute/verdict
//! record files it is read from.
//!
//! Everything here reads `.work/disputes/<stage>/<n>/*.md` on behalf of
//! [`super::AdjudicatorRegistry`]'s polling methods — it never writes.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::dispute::{DisputeRequest, DisputeVerdictRecord};

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
