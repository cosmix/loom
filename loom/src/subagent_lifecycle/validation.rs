use super::claude::{revalidate_claude_record, ClaudeEvidenceError};
use super::codex_evidence::validate_codex_record;
use super::model::{
    LifecycleProducer, LifecycleRecord, LifecycleState, WorkerOutcome, LIFECYCLE_VERSION,
};
use anyhow::{ensure, Result};
use std::path::Path;

use crate::models::forward_receipt::is_safe_id;

pub(super) enum Validation {
    Valid,
    Stale(String),
    Invalid(String),
}

pub(super) fn validate_record(work_dir: &Path, record: &LifecycleRecord) -> Validation {
    if record.version != LIFECYCLE_VERSION {
        return Validation::Invalid(format!("unknown lifecycle version {}", record.version));
    }
    if !valid_digest(&record.event_id) {
        return Validation::Invalid("malformed lifecycle event id".into());
    }
    match record.producer {
        LifecycleProducer::ClaudeSubagentStop | LifecycleProducer::ClaudeTeammateIdle => {
            validate_claude(work_dir, record)
        }
        LifecycleProducer::CodexCompanion | LifecycleProducer::CodexDirect => {
            validate_codex_record(work_dir, record).map_or_else(
                |error| Validation::Invalid(error.to_string()),
                |_| Validation::Valid,
            )
        }
    }
}

fn validate_claude(work_dir: &Path, record: &LifecycleRecord) -> Validation {
    match revalidate_claude_record(work_dir, record) {
        Ok(()) => Validation::Valid,
        Err(ClaudeEvidenceError::Stale(reason)) => Validation::Stale(reason),
        Err(error) => Validation::Invalid(error.to_string()),
    }
}

pub(super) fn fold_states(records: &[&LifecycleRecord]) -> WorkerOutcome {
    let mut terminal: Option<WorkerOutcome> = None;
    let mut active = false;
    for record in records {
        let next = match record.state {
            LifecycleState::Completed => Some(WorkerOutcome::Succeeded),
            LifecycleState::Failed => Some(WorkerOutcome::Failed(detail(record))),
            LifecycleState::Cancelled => Some(WorkerOutcome::Cancelled(detail(record))),
            LifecycleState::Unknown => Some(WorkerOutcome::Unknown(detail(record))),
            LifecycleState::TurnFinished | LifecycleState::Idle | LifecycleState::Running => {
                active = true;
                None
            }
        };
        if let Some(next) = next {
            if terminal.as_ref().is_some_and(|known| known != &next) {
                return WorkerOutcome::Unknown("contradictory terminal lifecycle evidence".into());
            }
            terminal = Some(next);
        }
    }
    terminal.unwrap_or_else(|| {
        if active {
            WorkerOutcome::Active
        } else {
            WorkerOutcome::Unknown("no lifecycle state".into())
        }
    })
}

fn detail(record: &LifecycleRecord) -> String {
    record
        .evidence
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("worker reported no detail")
        .to_owned()
}

pub(super) fn validate_safe_id(value: &str) -> Result<()> {
    ensure!(is_safe_id(value), "unsafe lifecycle identity");
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}
