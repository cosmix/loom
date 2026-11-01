use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::safe_read::{read_bounded, OverLimit};
use crate::handoff::completion::attestation_key;
use crate::handoff::schema::{CompletionCheckpoint, MAX_EVIDENCE_NONCES};
use crate::handoff::{HandoffOrigin, HandoffV2, ParsedHandoff};

use super::numbering::{find_latest_handoff, numbered_handoffs};
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
pub(super) fn find_matching_handoff_with_context(
    stage_id: &str,
    session_id: &str,
    origin: HandoffOrigin,
    context_tokens: u32,
    work_dir: &Path,
) -> Result<bool> {
    let mut has_context = false;
    let found = find_handoff_where(stage_id, work_dir, |handoff| {
        let matches = handoff.stage_id == stage_id
            && handoff.session_id == session_id
            && handoff.origin == Some(origin);
        if matches {
            has_context = handoff.context_tokens == context_tokens;
        }
        matches
    })?;
    Ok(found.is_some() && has_context)
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
/// Fold syntactically valid checkpoints without authenticating them; diagnostics only.
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
    let handoffs = session_handoffs(stage_id, session_id, work_dir)?;
    // Authentication must precede the lossy merge of latest evidence and nonce state.
    let trusted: Vec<_> = handoffs
        .iter()
        .filter_map(|(_, handoff)| {
            handoff
                .completion_checkpoint
                .as_ref()
                .and_then(|checkpoint| {
                    let trusted = checkpoint.trusted(&key);
                    (!trusted.observations.is_empty() || trusted.accepted.is_some())
                        .then_some(trusted)
                })
        })
        .collect();
    let mut merged = fold_checkpoints(trusted.iter())?;
    if let Some(checkpoint) = &mut merged {
        checkpoint.capacity_exhausted = checkpoint.observations.len() >= MAX_EVIDENCE_NONCES;
    }
    Ok(merged)
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
    mut matches: impl FnMut(&HandoffV2) -> bool,
) -> Result<Option<PathBuf>> {
    for (_, path) in numbered_handoffs(stage_id, work_dir)? {
        let Some(content) = read_handoff_artifact(work_dir, &path)? else {
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
    for (_, path) in numbered_handoffs(stage_id, work_dir)?.into_iter().rev() {
        let Some(content) = read_handoff_artifact(work_dir, &path)? else {
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

fn read_handoff_artifact(work_dir: &Path, path: &Path) -> Result<Option<String>> {
    let read = || -> Result<Vec<u8>> {
        // The work directory itself may be the intentional .loom/work symlink.
        let root = fs::canonicalize(work_dir)?;
        let relative = path.strip_prefix(work_dir)?;
        read_bounded(&root, relative, MAX_HANDOFF_BYTES)
    };
    let bytes = match read() {
        Ok(bytes) => bytes,
        Err(error) if error.downcast_ref::<OverLimit>().is_some() => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to read handoff file: {}", path.display()))
        }
    };
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

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
