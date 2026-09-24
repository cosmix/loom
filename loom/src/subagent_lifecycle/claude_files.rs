use super::claude::{ClaudeEvidenceError, TranscriptEvidence};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use crate::fs::stage_files::{extract_stage_id, find_stage_file};
use crate::models::forward_receipt::is_safe_id;
use crate::parser::frontmatter::extract_frontmatter_field;

const MAX_FINAL_RECORD_BYTES: u64 = 1024 * 1024;

pub(super) fn transcript_evidence(path: &Path) -> Result<TranscriptEvidence, ClaudeEvidenceError> {
    let (transcript_bytes, mut tail) = read_stable_transcript_tail(path)?;
    tail.pop();
    let line = final_record_line(&tail, transcript_bytes)?;
    let final_record = serde_json::from_slice::<Value>(line)
        .map_err(|error| ClaudeEvidenceError::Malformed(error.to_string()))?;
    if !final_record.is_object() {
        return Err(ClaudeEvidenceError::Malformed(
            "final transcript record is not an object".into(),
        ));
    }
    Ok(TranscriptEvidence {
        transcript_bytes,
        final_record_sha256: super::claude_event::digest(line),
    })
}

fn read_stable_transcript_tail(path: &Path) -> Result<(u64, Vec<u8>), ClaudeEvidenceError> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let before = file.metadata()?.len();
    if before == 0 {
        return Err(ClaudeEvidenceError::Malformed("empty transcript".into()));
    }
    let take = before.min(MAX_FINAL_RECORD_BYTES);
    file.seek(SeekFrom::Start(before - take))?;
    let capacity = usize::try_from(take)
        .map_err(|_| ClaudeEvidenceError::Malformed("transcript tail too large".into()))?;
    let mut tail = Vec::with_capacity(capacity);
    file.read_to_end(&mut tail)?;
    let after = file.metadata()?;
    let leaf = fs::symlink_metadata(path)?;
    if after.len() != before
        || after.dev() != leaf.dev()
        || after.ino() != leaf.ino()
        || tail.last() != Some(&b'\n')
    {
        return Err(ClaudeEvidenceError::Stale(
            "transcript is growing or torn".into(),
        ));
    }
    Ok((before, tail))
}

fn final_record_line(tail: &[u8], transcript_bytes: u64) -> Result<&[u8], ClaudeEvidenceError> {
    let take = transcript_bytes.min(MAX_FINAL_RECORD_BYTES);
    let start = tail
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let line = &tail[start..];
    if line.is_empty() || (transcript_bytes > take && start == 0) {
        return Err(ClaudeEvidenceError::Malformed(
            "final transcript record exceeds cap".into(),
        ));
    }
    Ok(line)
}

pub(super) fn plain_absolute(path: &Path) -> Result<PathBuf, ClaudeEvidenceError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(ClaudeEvidenceError::Unsafe(format!(
            "non-normal path: {}",
            path.display()
        )));
    }
    reject_symlink_components(path)?;
    if !fs::metadata(path)?.is_file() {
        return Err(ClaudeEvidenceError::Unsafe(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    let canonical = fs::canonicalize(path)?;
    if canonical != path {
        return Err(ClaudeEvidenceError::Unsafe(
            "path is not absolute-normalized".into(),
        ));
    }
    Ok(canonical)
}

/// Fail on the first component of `path` that is a symlink.
pub(crate) fn reject_symlink_components(path: &Path) -> Result<(), ClaudeEvidenceError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::RootDir) {
            continue;
        }
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(ClaudeEvidenceError::Unsafe(format!(
                "symlinked path: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_transcript_layout(
    worker: &Path,
    parent: &Path,
    parent_id: &str,
    agent_id: &str,
) -> Result<(), ClaudeEvidenceError> {
    let expected_name = format!("agent-{agent_id}.jsonl");
    let worker_session = worker
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|value| value.to_str());
    if worker.file_name().and_then(|value| value.to_str()) != Some(&expected_name)
        || worker
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            != Some("subagents")
        || worker_session != Some(parent_id)
    {
        return Err(ClaudeEvidenceError::Mismatch(
            "worker transcript layout or agent id".into(),
        ));
    }
    validate_parent_transcript(parent, parent_id)?;
    if expected_parent_transcript(worker, parent_id)? != parent {
        return Err(ClaudeEvidenceError::Mismatch(
            "parent and worker transcript roots differ".into(),
        ));
    }
    Ok(())
}

pub(super) fn expected_parent_transcript(
    worker: &Path,
    parent_id: &str,
) -> Result<PathBuf, ClaudeEvidenceError> {
    let project = worker
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| {
            ClaudeEvidenceError::Mismatch("worker transcript has no project root".into())
        })?;
    Ok(project.join(format!("{parent_id}.jsonl")))
}

pub(super) fn validate_parent_transcript(
    path: &Path,
    parent_id: &str,
) -> Result<(), ClaudeEvidenceError> {
    let expected = format!("{parent_id}.jsonl");
    if path.file_name().and_then(|value| value.to_str()) != Some(&expected) {
        return Err(ClaudeEvidenceError::Mismatch(
            "parent transcript basename differs from session".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_current_binding(
    work_dir: &Path,
    stage: &str,
    session: &str,
) -> Result<(), ClaudeEvidenceError> {
    let path = exact_stage_file(work_dir, stage)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut content = String::new();
    file.by_ref()
        .take(1024 * 1024)
        .read_to_string(&mut content)?;
    let (file_stage, file_session) = frontmatter_identity(&content)?;
    if file_stage != stage || file_session.as_deref() != Some(session) {
        return Err(ClaudeEvidenceError::Stale(
            "stage session binding changed".into(),
        ));
    }
    Ok(())
}

fn exact_stage_file(work_dir: &Path, stage: &str) -> Result<PathBuf, ClaudeEvidenceError> {
    let stages = work_dir.join("stages");
    let found = find_stage_file(&stages, stage).map_err(malformed)?;
    if matching_stage_file_count(&stages, stage)? != 1 {
        return Err(ClaudeEvidenceError::Mismatch(
            "stage file missing or ambiguous".into(),
        ));
    }
    let path = found
        .ok_or_else(|| ClaudeEvidenceError::Mismatch("stage file missing or ambiguous".into()))?;
    plain_absolute(&path)
}

fn matching_stage_file_count(stages: &Path, stage: &str) -> Result<usize, ClaudeEvidenceError> {
    let mut count = 0;
    for entry in fs::read_dir(stages)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if path.extension().and_then(|value| value.to_str()) == Some("md")
            && extract_stage_id(name).as_deref() == Some(stage)
        {
            count += 1;
        }
    }
    Ok(count)
}

fn frontmatter_identity(content: &str) -> Result<(String, Option<String>), ClaudeEvidenceError> {
    let stage = identity_field(content, "id")?
        .ok_or_else(|| ClaudeEvidenceError::Malformed("stage frontmatter has no id".into()))?;
    let session = identity_field(content, "session")?;
    Ok((stage, session))
}

fn identity_field(content: &str, field: &str) -> Result<Option<String>, ClaudeEvidenceError> {
    let value = extract_frontmatter_field(content, field).map_err(malformed)?;
    if value.as_deref().is_some_and(|value| !is_safe_id(value)) {
        return Err(ClaudeEvidenceError::Malformed(
            "unsafe stage identity field".into(),
        ));
    }
    Ok(value)
}

fn malformed(error: impl std::fmt::Display) -> ClaudeEvidenceError {
    ClaudeEvidenceError::Malformed(error.to_string())
}
