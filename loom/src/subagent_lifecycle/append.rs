//! Appends to a stage's lifecycle journal under its [`JournalLock`].

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

use super::lock::JournalLock;
use super::model::LifecycleRecord;
use super::store::{is_not_found, read_journal, AppendOutcome};
use super::validation::validate_safe_id;

#[cfg(test)]
pub(crate) fn append_locked(work_dir: &Path, record: &LifecycleRecord) -> Result<AppendOutcome> {
    let outcomes = append_all_locked(work_dir, &[record], |_| Ok(true))?;
    outcomes
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("lifecycle append returned no outcome"))
}

/// Append `records`, all for one stage, under a single journal lock. `admit`
/// sees the journal as it is under that lock and decides whether anything is
/// appended: `Ok(false)` appends nothing and reports every record as a
/// [`AppendOutcome::Duplicate`], an error aborts the append. Each record's
/// duplicate check sees the records appended before it in the same call.
pub(crate) fn append_all_locked(
    work_dir: &Path,
    records: &[&LifecycleRecord],
    admit: impl FnOnce(&[LifecycleRecord]) -> Result<bool>,
) -> Result<Vec<AppendOutcome>> {
    let stage_id = records
        .first()
        .map(|record| record.identity.stage_id())
        .ok_or_else(|| anyhow::anyhow!("no lifecycle records to append"))?;
    ensure!(
        records
            .iter()
            .all(|record| record.identity.stage_id() == stage_id),
        "lifecycle records span stages"
    );
    validate_safe_id(stage_id)?;
    let rel_dir = PathBuf::from("subagents").join(stage_id);
    crate::fs::safe_fs::safe_create_dir_all(work_dir, &rel_dir, 0o700)?;
    let relpath = rel_dir.join("lifecycle.jsonl");
    let journal = work_dir.join(&relpath);
    let _lock = JournalLock::acquire(&journal)?;
    let mut existing = match read_journal(&journal) {
        Ok(records) => records,
        Err(error) if is_not_found(&error) => Vec::new(),
        Err(error) => return Err(error),
    };
    ensure!(
        !journal_lacks_final_newline(&journal)?,
        "refusing to append after a torn lifecycle line"
    );
    if !admit(&existing)? {
        return Ok(vec![AppendOutcome::Duplicate; records.len()]);
    }
    let mut outcomes = Vec::with_capacity(records.len());
    for record in records {
        let outcome = duplicate_outcome(&existing, record);
        if outcome != AppendOutcome::Duplicate {
            let mut line = serde_json::to_vec(record)?;
            line.push(b'\n');
            crate::fs::safe_fs::safe_append(work_dir, &relpath, &line)?;
            existing.push((*record).clone());
        }
        outcomes.push(outcome);
    }
    Ok(outcomes)
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
