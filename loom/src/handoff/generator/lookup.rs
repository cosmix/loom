use std::cmp::Reverse;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::handoff::completion::attestation_key;
use crate::handoff::schema::CompletionCheckpoint;
use crate::handoff::{HandoffOrigin, HandoffV2, ParsedHandoff};

use super::numbering::find_latest_handoff;
const MAX_HANDOFF_BYTES: usize = 1024 * 1024;
pub fn find_matching_handoff(
    stage_id: &str,
    session_id: &str,
    origin: HandoffOrigin,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    find_handoff_where(stage_id, work_dir, |handoff| {
        handoff.stage_id == stage_id
            && handoff.session_id == session_id
            && handoff.origin == Some(origin)
    })
}
pub fn find_latest_session_handoff(
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    find_handoff_where(stage_id, work_dir, |handoff| {
        handoff.stage_id == stage_id && handoff.session_id == session_id
    })
}
pub fn find_continuation_handoff(
    stage_id: &str,
    outgoing_session_id: Option<&str>,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    match outgoing_session_id {
        Some(session_id) => Ok(session_handoffs(stage_id, session_id, work_dir)?
            .into_iter()
            .max_by_key(|(path, handoff)| continuation_richness(path, handoff))
            .map(|(path, _)| path)),
        None => find_latest_handoff(stage_id, work_dir),
    }
}
pub fn load_session_checkpoint(
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<CompletionCheckpoint>> {
    let handoffs = session_handoffs(stage_id, session_id, work_dir)?;
    fold_checkpoints(
        handoffs
            .iter()
            .filter_map(|(_, handoff)| handoff.completion_checkpoint.as_ref()),
    )
}
pub fn load_trusted_session_checkpoint(
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<CompletionCheckpoint>> {
    let key = attestation_key(work_dir)?;
    let Some(checkpoint) = load_session_checkpoint(stage_id, session_id, work_dir)? else {
        return Ok(None);
    };
    let trusted = checkpoint.trusted(&key);
    if trusted.observations.is_empty() && trusted.accepted.is_none() {
        return Ok(None);
    }
    Ok(Some(trusted))
}
pub(super) fn fold_checkpoints<'a, I>(checkpoints: I) -> Result<Option<CompletionCheckpoint>>
where
    I: IntoIterator<Item = &'a CompletionCheckpoint>,
{
    let mut merged: Option<CompletionCheckpoint> = None;
    for checkpoint in checkpoints {
        match &mut merged {
            Some(existing) => {
                existing.merge_from(checkpoint)?;
            }
            None => merged = Some(checkpoint.clone()),
        }
    }
    Ok(merged)
}
pub fn find_continuation_handoff_name(
    stage_id: &str,
    outgoing_session_id: Option<&str>,
    work_dir: &Path,
) -> Result<Option<String>> {
    Ok(
        find_continuation_handoff(stage_id, outgoing_session_id, work_dir)?.and_then(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str().map(str::to_owned))
        }),
    )
}
fn find_handoff_where(
    stage_id: &str,
    work_dir: &Path,
    matches: impl Fn(&HandoffV2) -> bool,
) -> Result<Option<PathBuf>> {
    for path in numbered_handoff_paths(stage_id, work_dir)? {
        let Some(content) = read_handoff_artifact(&path)? else {
            continue;
        };
        let parsed = ParsedHandoff::parse(&content);
        let Some(handoff) = parsed.as_v2() else {
            continue;
        };
        if matches(handoff) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
pub(super) fn session_handoffs(
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
) -> Result<Vec<(PathBuf, HandoffV2)>> {
    let mut handoffs = Vec::new();
    for path in numbered_handoff_paths(stage_id, work_dir)?
        .into_iter()
        .rev()
    {
        let Some(content) = read_handoff_artifact(&path)? else {
            continue;
        };
        let parsed = ParsedHandoff::parse(&content);
        let Some(handoff) = parsed.as_v2() else {
            continue;
        };
        if handoff.stage_id == stage_id
            && handoff.session_id == session_id
            && handoff.validate().is_ok()
        {
            handoffs.push((path, handoff.clone()));
        }
    }
    Ok(handoffs)
}

fn read_handoff_artifact(path: &Path) -> Result<Option<String>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to read handoff file: {}", path.display()))?;
    let mut bytes = Vec::with_capacity(MAX_HANDOFF_BYTES + 1);
    file.take((MAX_HANDOFF_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read handoff file: {}", path.display()))?;
    if bytes.len() > MAX_HANDOFF_BYTES {
        return Ok(None);
    }
    Ok(String::from_utf8(bytes).ok())
}

fn continuation_richness(path: &Path, handoff: &HandoffV2) -> (usize, usize, u32) {
    let observations = handoff
        .completion_checkpoint
        .as_ref()
        .map_or(0, |checkpoint| checkpoint.observations.len());
    let completed_work =
        handoff.completed_tasks.len() + handoff.commits.len() + handoff.files_modified.len();
    (observations, completed_work, handoff_number(path))
}

fn handoff_number(path: &Path) -> u32 {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit('-').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_default()
}

fn numbered_handoff_paths(stage_id: &str, work_dir: &Path) -> Result<Vec<PathBuf>> {
    let handoffs_dir = work_dir.join("handoffs");
    let entries = match fs::read_dir(&handoffs_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "Failed to read handoffs directory: {}",
                    handoffs_dir.display()
                )
            });
        }
    };

    let prefix = format!("{stage_id}-handoff-");
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.context("Failed to read handoff directory entry")?;
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        let Some(number) = filename
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".md"))
            .and_then(|number| number.parse::<u32>().ok())
        else {
            continue;
        };
        candidates.push((number, entry.path()));
    }
    candidates.sort_unstable_by_key(|(number, _)| Reverse(*number));
    Ok(candidates.into_iter().map(|(_, path)| path).collect())
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
