//! Host-owned record directory: creation, event appends and bounded reads.

use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_create_new_dir_in_workdir, safe_create_new_in_workdir,
    safe_open_dirfd, safe_rename_in_workdir,
};
use crate::utils::sha256_hex;

use super::ids::{
    validate_invocation_id, validate_message_id, validate_stage_id, validate_unit_id,
};
use super::model::{
    BoundaryIdentity, CancelRequest, ControlMessage, EventRecord, NativeEvent, NativeManifest,
    ReportInfo, MAX_BRIEF_BYTES, MAX_MESSAGE_BYTES, MAX_RECORD_BYTES,
};
use super::store_io::{io_err, open_append, publish, read_json, read_regular, write_json};

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Corrupt(String),
    TooLarge(&'static str),
    UnsafePath(String),
    AlreadyExists,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Corrupt(detail) => write!(f, "{detail}"),
            Self::TooLarge(name) => write!(f, "{name} exceeds the size limit"),
            Self::UnsafePath(detail) => write!(f, "unsafe path: {detail}"),
            Self::AlreadyExists => write!(f, "invocation already exists"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MessageWrite {
    Stored,
    AlreadyStored,
    Conflict,
}

pub fn stage_dir(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join("native-workers").join(stage_id)
}

pub fn invocation_dir(work_dir: &Path, stage_id: &str, invocation_id: &str) -> PathBuf {
    stage_dir(work_dir, stage_id).join(invocation_id)
}

pub fn read_manifest(dir: &Path) -> Result<NativeManifest, StoreError> {
    let manifest: NativeManifest = read_json(dir, "manifest.json")?
        .ok_or_else(|| StoreError::Corrupt("manifest.json is missing".into()))?;
    if manifest.v != 1 {
        return Err(StoreError::Corrupt("unsupported manifest version".into()));
    }
    Ok(manifest)
}

pub fn read_boundary(dir: &Path) -> Result<Option<BoundaryIdentity>, StoreError> {
    read_json(dir, "boundary.json")
}

pub fn write_boundary(dir: &Path, identity: &BoundaryIdentity) -> Result<(), StoreError> {
    write_json(dir, "boundary.json", identity)
}

/// Created by the supervisor before `boundary::spawn`. From then on a namespace may
/// exist that no `boundary.json` names yet, so a lost supervisor's record whose
/// `boundary.json` is absent cannot be released without proof.
const SPAWN_MARKER: &str = "boundary.pending";

pub fn mark_spawn_begun(dir: &Path) -> Result<(), StoreError> {
    let fd = safe_open_dirfd(dir).map_err(io_err)?;
    safe_create_new_in_workdir(fd.as_raw_fd(), Path::new(SPAWN_MARKER), b"").map_err(io_err)?;
    File::from(fd).sync_all()?;
    Ok(())
}

/// Whether `mark_spawn_begun` ran. Anything at the path other than a clean
/// absence counts as begun: the answer only ever withholds a release.
pub fn spawn_begun(dir: &Path) -> bool {
    !matches!(fs::symlink_metadata(dir.join(SPAWN_MARKER)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

pub fn read_cancel(dir: &Path) -> Result<Option<CancelRequest>, StoreError> {
    read_json(&dir.join("control"), "cancel.json")
}

pub fn write_cancel(dir: &Path, request: &CancelRequest) -> Result<(), StoreError> {
    if read_cancel(dir)?.is_some() {
        return Ok(());
    }
    write_json(&dir.join("control"), "cancel.json", request)
}

pub fn read_messages(dir: &Path) -> Result<Vec<ControlMessage>, StoreError> {
    let control = dir.join("control");
    let entries = match fs::read_dir(&control) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut messages = Vec::new();
    for entry in entries {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name
            .strip_prefix("msg-")
            .and_then(|s| s.strip_suffix(".json"))
        else {
            continue;
        };
        if validate_message_id(id).is_err() {
            continue;
        }
        let Some(message): Option<ControlMessage> = read_json(&control, name)? else {
            continue;
        };
        if message.message_id != id {
            return Err(StoreError::Corrupt(format!(
                "{name}: message id differs from filename"
            )));
        }
        messages.push(message);
    }
    messages.sort_by(|a, b| (a.accepted_at, &a.message_id).cmp(&(b.accepted_at, &b.message_id)));
    Ok(messages)
}

pub fn write_message(dir: &Path, message: &ControlMessage) -> Result<MessageWrite, StoreError> {
    validate_message_id(&message.message_id)
        .map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    if message.text.len() > MAX_MESSAGE_BYTES {
        return Err(StoreError::TooLarge("message"));
    }
    let control = dir.join("control");
    let name = format!("msg-{}.json", message.message_id);
    if let Some(existing) = read_json::<ControlMessage>(&control, &name)? {
        return Ok(if existing.sha256 == message.sha256 {
            MessageWrite::AlreadyStored
        } else {
            MessageWrite::Conflict
        });
    }
    write_json(&control, &name, message)?;
    Ok(MessageWrite::Stored)
}

pub fn write_report(dir: &Path, text: &str) -> Result<ReportInfo, StoreError> {
    if text.len() as u64 > MAX_RECORD_BYTES {
        return Err(StoreError::TooLarge("report"));
    }
    publish(dir, "report.md", text.as_bytes())?;
    Ok(ReportInfo {
        bytes: text.len() as u64,
        sha256: sha256_hex(text.as_bytes()),
        complete: false,
    })
}

pub fn read_report(dir: &Path, max_bytes: usize) -> Result<Option<(String, bool)>, StoreError> {
    let Some(bytes) = read_regular(dir, "report.md", MAX_RECORD_BYTES)? else {
        return Ok(None);
    };
    let mut text = String::from_utf8(bytes)
        .map_err(|error| StoreError::Corrupt(format!("report.md: {error}")))?;
    let truncated = text.len() > max_bytes;
    if truncated {
        text.truncate(text.floor_char_boundary(max_bytes));
    }
    Ok(Some((text, truncated)))
}

pub fn read_events(dir: &Path) -> Result<Vec<EventRecord>, StoreError> {
    let bytes = read_regular(dir, "events.jsonl", MAX_RECORD_BYTES)?
        .ok_or_else(|| StoreError::Corrupt("events.jsonl is missing".into()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| StoreError::Corrupt(format!("events.jsonl: {error}")))?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(StoreError::Corrupt(format!(
            "line {}: unterminated last line",
            text.lines().count()
        )));
    }
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let n = index as u64 + 1;
            let record: EventRecord = serde_json::from_str(line)
                .map_err(|error| StoreError::Corrupt(format!("line {n}: {error}")))?;
            if record.seq != n {
                return Err(StoreError::Corrupt(format!(
                    "line {n}: expected sequence {n}, got {}",
                    record.seq
                )));
            }
            Ok(record)
        })
        .collect()
}

/// Single writer; ownership is transferred only after the previous writer stops.
pub struct EventLog {
    file: File,
    next_seq: u64,
    bytes: u64,
    poisoned: bool,
}

impl EventLog {
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let events = read_events(dir)?;
        let next_seq = events.last().map_or(1, |record| record.seq + 1);
        let file = open_append(dir)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            file,
            next_seq,
            bytes,
            poisoned: false,
        })
    }

    pub fn append(&mut self, event: NativeEvent) -> Result<EventRecord, StoreError> {
        if self.poisoned {
            return Err(StoreError::Corrupt(
                "event log poisoned by a failed append".into(),
            ));
        }
        let record = EventRecord {
            seq: self.next_seq,
            at: Utc::now(),
            event,
        };
        let mut line = serde_json::to_string(&record)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        line.push('\n');
        if self.bytes + line.len() as u64 > MAX_RECORD_BYTES {
            return Err(StoreError::TooLarge("events.jsonl"));
        }
        let result = self
            .file
            .write_all(line.as_bytes())
            .and_then(|()| self.file.sync_all());
        if let Err(error) = result {
            self.poisoned = true;
            return Err(StoreError::Io(error));
        }
        self.next_seq += 1;
        self.bytes += line.len() as u64;
        Ok(record)
    }
}

pub fn create_invocation(
    work_dir: &Path,
    manifest: &NativeManifest,
    brief: &[u8],
) -> Result<PathBuf, StoreError> {
    validate_creation(manifest, brief)?;
    let rel = Path::new("native-workers").join(&manifest.stage_id);
    let fd = safe_open_dirfd(work_dir).map_err(io_err)?;
    safe_create_dir_all_in_workdir(fd.as_raw_fd(), &rel, 0o700).map_err(io_err)?;
    let final_rel = rel.join(&manifest.invocation_id);
    match fs::symlink_metadata(work_dir.join(&final_rel)) {
        Ok(_) => return Err(StoreError::AlreadyExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let tmp_rel = rel.join(format!(".{}.tmp", manifest.invocation_id));
    safe_create_new_dir_in_workdir(fd.as_raw_fd(), &tmp_rel, 0o700).map_err(io_err)?;
    let control_rel = tmp_rel.join("control");
    safe_create_new_dir_in_workdir(fd.as_raw_fd(), &control_rel, 0o700).map_err(io_err)?;
    let tmp = work_dir.join(&tmp_rel);
    write_json(&tmp, "manifest.json", manifest)?;
    publish(&tmp, "brief.md", brief)?;
    let tmpfd = safe_open_dirfd(&tmp).map_err(io_err)?;
    safe_create_new_in_workdir(tmpfd.as_raw_fd(), Path::new("events.jsonl"), b"")
        .map_err(io_err)?;
    EventLog::open(&tmp)?.append(NativeEvent::Admitted)?;
    safe_rename_in_workdir(fd.as_raw_fd(), &tmp_rel, &final_rel).map_err(io_err)?;
    let stagefd = safe_open_dirfd(&stage_dir(work_dir, &manifest.stage_id)).map_err(io_err)?;
    File::from(stagefd).sync_all()?;
    Ok(invocation_dir(
        work_dir,
        &manifest.stage_id,
        &manifest.invocation_id,
    ))
}

fn validate_creation(manifest: &NativeManifest, brief: &[u8]) -> Result<(), StoreError> {
    validate_stage_id(&manifest.stage_id)
        .map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    validate_unit_id(&manifest.unit_id)
        .map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    validate_invocation_id(&manifest.invocation_id)
        .map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    if brief.len() as u64 > MAX_BRIEF_BYTES {
        return Err(StoreError::TooLarge("brief"));
    }
    if manifest.v != 1 {
        return Err(StoreError::Corrupt("unsupported manifest version".into()));
    }
    if manifest.brief_sha256 != sha256_hex(brief) {
        return Err(StoreError::Corrupt(
            "brief sha256 differs from manifest".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
