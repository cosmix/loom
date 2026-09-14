pub(crate) use super::codex_evidence::codex_event_id;
use super::codex_evidence::codex_records_outcome;
use super::lock::JournalLock;
use super::model::{LifecycleRecord, WorkerIdentity, WorkerOutcome};
use super::validation::{fold_states, validate_record, validate_safe_id, Validation};
use anyhow::{ensure, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Appended,
    Duplicate,
    Conflict,
}

#[derive(Debug, Default)]
pub struct LifecycleIndex {
    work_dir: PathBuf,
    records: Vec<LifecycleRecord>,
    conflicted: HashSet<WorkerIdentity>,
    corrupt_stages: HashMap<String, String>,
}

pub(crate) fn append_locked(work_dir: &Path, record: &LifecycleRecord) -> Result<AppendOutcome> {
    validate_safe_id(record.identity.stage_id())?;
    let rel_dir = PathBuf::from("subagents").join(record.identity.stage_id());
    crate::fs::safe_fs::safe_create_dir_all(work_dir, &rel_dir, 0o700)?;
    let relpath = rel_dir.join("lifecycle.jsonl");
    let journal = work_dir.join(&relpath);
    let _lock = JournalLock::acquire(&journal)?;
    let existing = match read_journal(&journal) {
        Ok(records) => records,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            Vec::new()
        }
        Err(error) => return Err(error),
    };
    ensure!(
        !journal_lacks_final_newline(&journal)?,
        "refusing to append after a torn lifecycle line"
    );
    let outcome = duplicate_outcome(&existing, record);
    if outcome != AppendOutcome::Duplicate {
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        crate::fs::safe_fs::safe_append(work_dir, &relpath, &line)?;
    }
    Ok(outcome)
}

pub fn replay(work_dir: &Path) -> Result<LifecycleIndex> {
    let mut index = LifecycleIndex {
        work_dir: work_dir.to_path_buf(),
        ..LifecycleIndex::default()
    };
    let root = work_dir.join("subagents");
    if let Ok(metadata) = fs::symlink_metadata(&root) {
        ensure!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "lifecycle subagents root is not a plain directory"
        );
    }
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(index),
        Err(error) => return Err(error).context("reading lifecycle stage directories"),
    };
    let mut by_event: HashMap<String, LifecycleRecord> = HashMap::new();
    for entry in entries {
        replay_stage(entry?.path(), &mut index, &mut by_event)?;
    }
    index.records = by_event.into_values().collect();
    Ok(index)
}

impl LifecycleIndex {
    pub fn claude_outcome(
        &self,
        stage_id: &str,
        loom_session_id: &str,
        parent_session_id: &str,
        agent_id: &str,
        transcript_path: &Path,
    ) -> WorkerOutcome {
        let matches: Vec<_> = self
            .records
            .iter()
            .filter(|record| match &record.identity {
                WorkerIdentity::ClaudeSubagent {
                    stage_id: stage,
                    loom_session_id: loom,
                    parent_session_id: parent,
                    agent_id: agent,
                    transcript_path: transcript,
                    ..
                } => {
                    stage == stage_id
                        && loom == loom_session_id
                        && parent == parent_session_id
                        && agent == agent_id
                        && transcript == transcript_path
                }
                _ => false,
            })
            .collect();
        self.records_outcome(stage_id, &matches, false)
    }

    pub fn forwarded_outcome(
        &self,
        stage_id: &str,
        loom_session_id: &str,
        parent_session_id: &str,
        forwarder_agent_id: &str,
    ) -> WorkerOutcome {
        let identities: HashSet<_> = self
            .records
            .iter()
            .filter_map(|record| match &record.identity {
                WorkerIdentity::Codex {
                    stage_id: stage,
                    loom_session_id: loom,
                    parent_session_id: parent,
                    forwarder_agent_id: forwarder,
                    ..
                } if stage == stage_id
                    && loom == loom_session_id
                    && parent == parent_session_id
                    && forwarder == forwarder_agent_id =>
                {
                    Some(record.identity.clone())
                }
                _ => None,
            })
            .collect();
        if identities.len() != 1 {
            return WorkerOutcome::Unknown("missing or conflicting Codex authorization".into());
        }
        let Some(identity) = identities.into_iter().next() else {
            return WorkerOutcome::Unknown("missing Codex authorization".into());
        };
        self.outcome(&identity)
    }

    pub fn outcome(&self, identity: &WorkerIdentity) -> WorkerOutcome {
        let records: Vec<_> = self
            .records
            .iter()
            .filter(|record| &record.identity == identity)
            .collect();
        self.records_outcome(
            identity.stage_id(),
            &records,
            matches!(identity, WorkerIdentity::Codex { .. }),
        )
    }

    fn records_outcome(
        &self,
        stage_id: &str,
        records: &[&LifecycleRecord],
        require_authorization: bool,
    ) -> WorkerOutcome {
        if let Some(reason) = self.corrupt_stages.get(stage_id) {
            return WorkerOutcome::Unknown(reason.clone());
        }
        if records.is_empty() {
            return WorkerOutcome::Unknown("no matching lifecycle evidence".into());
        }
        if records
            .iter()
            .any(|record| self.conflicted.contains(&record.identity))
        {
            return WorkerOutcome::Unknown("conflicting lifecycle event id".into());
        }
        let mut valid = Vec::new();
        let mut invalid = None;
        for record in records {
            match validate_record(&self.work_dir, record) {
                Validation::Valid => valid.push(*record),
                Validation::Stale(reason) => {
                    invalid.get_or_insert(reason);
                }
                Validation::Invalid(reason) => return WorkerOutcome::Unknown(reason),
            }
        }
        if valid.is_empty() {
            return WorkerOutcome::Unknown(
                invalid.unwrap_or_else(|| "no valid lifecycle evidence".into()),
            );
        }
        if require_authorization {
            return codex_records_outcome(&valid);
        }
        fold_states(&valid)
    }
}

fn replay_stage(
    stage_dir: PathBuf,
    index: &mut LifecycleIndex,
    by_event: &mut HashMap<String, LifecycleRecord>,
) -> Result<()> {
    let Some(stage) = stage_dir.file_name().and_then(|value| value.to_str()) else {
        return Ok(());
    };
    if validate_safe_id(stage).is_err() {
        return Ok(());
    }
    let Some(loaded) = load_stage_records(&stage_dir, stage, index)? else {
        return Ok(());
    };
    for record in loaded {
        if record.identity.stage_id() != stage {
            index
                .corrupt_stages
                .insert(stage.into(), "cross-stage lifecycle record".into());
            continue;
        }
        match by_event.get(&record.event_id) {
            Some(existing) if existing == &record => {}
            Some(existing) => {
                index.conflicted.insert(existing.identity.clone());
                index.conflicted.insert(record.identity.clone());
            }
            None => {
                by_event.insert(record.event_id.clone(), record);
            }
        }
    }
    Ok(())
}

fn load_stage_records(
    stage_dir: &Path,
    stage: &str,
    index: &mut LifecycleIndex,
) -> Result<Option<Vec<LifecycleRecord>>> {
    let metadata = fs::symlink_metadata(stage_dir)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        index.corrupt_stages.insert(
            stage.into(),
            "lifecycle stage directory is not plain".into(),
        );
        return Ok(None);
    }
    let journal = stage_dir.join("lifecycle.jsonl");
    let loaded = match read_journal(&journal) {
        Ok(loaded) => loaded,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None)
        }
        Err(error) => {
            index.corrupt_stages.insert(stage.into(), error.to_string());
            return Ok(None);
        }
    };
    Ok(Some(loaded))
}

fn read_journal(path: &Path) -> Result<Vec<LifecycleRecord>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "lifecycle journal is not a regular file"
    );
    ensure!(
        metadata.len() <= MAX_JOURNAL_BYTES,
        "lifecycle journal exceeds read cap"
    );
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len())?);
    file.by_ref()
        .take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let terminated = bytes.last() == Some(&b'\n');
    ensure!(
        u64::try_from(bytes.len())? <= MAX_JOURNAL_BYTES,
        "lifecycle journal grew beyond read cap"
    );
    let parts: Vec<_> = bytes.split(|byte| *byte == b'\n').collect();
    let mut records = Vec::new();
    for (index, line) in parts.iter().enumerate() {
        let final_part = index + 1 == parts.len();
        if (!terminated && final_part) || (terminated && final_part && line.is_empty()) {
            continue;
        }
        let value = serde_json::from_slice::<serde_json::Value>(line)
            .context("malformed complete lifecycle line")?;
        let record = serde_json::from_value(value)
            .context("invalid lifecycle schema on complete JSON line")?;
        records.push(record);
    }
    Ok(records)
}

fn journal_lacks_final_newline(path: &Path) -> Result<bool> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)?;
    Ok(byte[0] != b'\n')
}

fn duplicate_outcome(existing: &[LifecycleRecord], incoming: &LifecycleRecord) -> AppendOutcome {
    let mut matching = existing
        .iter()
        .filter(|record| record.event_id == incoming.event_id);
    let Some(first) = matching.next() else {
        return AppendOutcome::Appended;
    };
    if first == incoming && matching.all(|record| record == incoming) {
        AppendOutcome::Duplicate
    } else {
        AppendOutcome::Conflict
    }
}
