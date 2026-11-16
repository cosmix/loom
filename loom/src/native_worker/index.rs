//! Read-only queries over native worker records: folded state, stage disposition, threads.

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::ids::{validate_invocation_id, validate_stage_id};
use super::model::{
    EventRecord, MessageInfo, NativeEvent, NativeManifest, NativeUnitState, Ownership, Phase,
    ReportInfo,
};
use super::store::{invocation_dir, read_events, read_manifest, stage_dir, StoreError};

#[derive(Debug, PartialEq, Eq)]
pub enum NativeDisposition {
    None,
    Active(Vec<String>),
    Unverified(String),
}

impl NativeDisposition {
    pub fn blocker(&self, stage_id: &str) -> Option<String> {
        match self {
            Self::None => None,
            Self::Active(units) => Some(format!(
                "stage '{stage_id}' has running native Codex workers ({}); cancel each with 'loom subagents cancel --stage {stage_id} --worker codex:<unit> --reason <text>' and wait for it to stop",
                units.join(", ")
            )),
            Self::Unverified(cause) if is_unproven_ownership(cause) => Some(format!(
                "stage '{stage_id}' has a native Codex worker whose processes could not be proven stopped ({cause}); inspect the worktree, then run 'loom subagents release --stage {stage_id} --invocation <invocation id>'"
            )),
            // `release` cannot read a damaged record, so the remedy is manual.
            Self::Unverified(cause) => Some(format!(
                "stage '{stage_id}' has a damaged native Codex worker record ({cause}); confirm no process of that worker still runs, then remove the damaged path under .loom/work/native-workers/{stage_id}/"
            )),
        }
    }
}

/// The cause suffix of an intact record whose ownership is `Unverified`; every other
/// `Unverified` cause comes from a damaged record or stage directory.
const UNPROVEN_OWNERSHIP: &str = ": processes not proven stopped";

fn is_unproven_ownership(cause: &str) -> bool {
    cause
        .strip_suffix(UNPROVEN_OWNERSHIP)
        .is_some_and(|id| validate_invocation_id(id).is_ok())
}

#[derive(Debug, PartialEq, Eq)]
pub struct NativeThread {
    pub thread_id: String,
    pub unit_id: String,
    pub invocation_id: String,
    pub requested_model: String,
    pub requested_effort: String,
    pub started_at: DateTime<Utc>,
}

pub fn fold(manifest: NativeManifest, events: &[EventRecord]) -> NativeUnitState {
    let mut state = NativeUnitState {
        last_progress_at: manifest.created_at,
        manifest,
        phase: Phase::Admitted,
        outcome: None,
        outcome_reason: None,
        ownership: Ownership::Held,
        thread_id: None,
        sandbox_mode: None,
        cancel_requested: false,
        report: None,
        messages: Vec::new(),
    };
    for record in events {
        state.last_progress_at = record.at;
        state.phase = state.phase.max(event_phase(&record.event));
        apply_event(&mut state, &record.event);
    }
    state
}

fn event_phase(event: &NativeEvent) -> Phase {
    match event {
        NativeEvent::BoundaryCreated { .. } => Phase::Starting,
        NativeEvent::TurnStarted { .. } => Phase::Running,
        NativeEvent::CancelRequested { .. }
        | NativeEvent::DeadlineExpired
        | NativeEvent::ReportStored { .. } => Phase::Finalizing,
        NativeEvent::Terminal { .. } => Phase::Terminal,
        _ => Phase::Admitted,
    }
}

fn apply_event(state: &mut NativeUnitState, event: &NativeEvent) {
    match event {
        NativeEvent::BoundaryCreated { sandbox_mode } => state.sandbox_mode = Some(*sandbox_mode),
        NativeEvent::ThreadStarted { thread_id } => state.thread_id = Some(thread_id.clone()),
        NativeEvent::CancelRequested { .. } => state.cancel_requested = true,
        NativeEvent::ReportStored {
            bytes,
            sha256,
            complete,
        } => {
            state.report = Some(ReportInfo {
                bytes: *bytes,
                sha256: sha256.clone(),
                complete: *complete,
            });
        }
        NativeEvent::Message {
            message_id,
            state: delivery,
            turn_id,
            ..
        } => {
            fold_message(&mut state.messages, message_id, *delivery, turn_id);
        }
        NativeEvent::Terminal { outcome, reason } if state.outcome.is_none() => {
            state.outcome = Some(*outcome);
            state.outcome_reason = Some(reason.clone());
        }
        NativeEvent::OwnershipReleased { by }
            if state.ownership == Ownership::Held
                || (state.ownership == Ownership::Unverified && by == "operator") =>
        {
            state.ownership = Ownership::Released;
        }
        NativeEvent::OwnershipUnverified { .. } if state.ownership == Ownership::Held => {
            state.ownership = Ownership::Unverified;
        }
        _ => {}
    }
}

fn fold_message(
    messages: &mut Vec<MessageInfo>,
    id: &str,
    state: super::model::MessageState,
    turn_id: &Option<String>,
) {
    if let Some(message) = messages.iter_mut().find(|message| message.message_id == id) {
        message.state = state;
        if turn_id.is_some() {
            message.turn_id.clone_from(turn_id);
        }
    } else {
        messages.push(MessageInfo {
            message_id: id.into(),
            state,
            turn_id: turn_id.clone(),
        });
    }
}

type LoadedUnit = (NativeUnitState, Vec<EventRecord>);

fn list_invocations(dir: &Path) -> Vec<Result<fs::DirEntry, StoreError>> {
    // A symlinked stage is damaged state, including when its target is absent.
    match fs::symlink_metadata(dir) {
        Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
            return vec![Err(StoreError::UnsafePath(
                "stage is not a real directory".into(),
            ))];
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![Err(error.into())],
        _ => {}
    }
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_name().to_string_lossy().starts_with('.') => None,
                other => Some(other.map_err(StoreError::Io)),
            })
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => vec![Err(error.into())],
    }
}

fn load_one(entry: fs::DirEntry, stage_id: &str) -> Result<LoadedUnit, StoreError> {
    load_named(&entry.path(), &entry.file_name(), stage_id)
}

fn load_named(path: &Path, name: &OsStr, stage_id: &str) -> Result<LoadedUnit, StoreError> {
    let load = || {
        let id = name
            .to_str()
            .ok_or_else(|| StoreError::Corrupt("non-UTF-8 name".into()))?;
        validate_invocation_id(id).map_err(|error| StoreError::UnsafePath(error.to_string()))?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(StoreError::UnsafePath(
                "invocation is not a real directory".into(),
            ));
        }
        let manifest = read_manifest(path)?;
        if manifest.stage_id != stage_id || manifest.invocation_id != id {
            return Err(StoreError::Corrupt(
                "manifest ids differ from directory names".into(),
            ));
        }
        let events = read_events(path)?;
        let first = events.first().map(|record| &record.event);
        if !matches!(first, Some(NativeEvent::Admitted)) {
            return Err(StoreError::Corrupt("first event is not Admitted".into()));
        }
        Ok((fold(manifest, &events), events))
    };
    load().map_err(|error| {
        let name = name.to_string_lossy();
        if validate_invocation_id(&name).is_ok() {
            StoreError::Corrupt(format!("{name}: {error}"))
        } else {
            StoreError::Corrupt(format!(
                "stage {stage_id}: unexpected entry {name}: {error}"
            ))
        }
    })
}

/// One invocation's state, read without its siblings: `Ok(None)` when it has no record.
pub fn invocation_state(
    work_dir: &Path,
    stage_id: &str,
    invocation_id: &str,
) -> Result<Option<NativeUnitState>, StoreError> {
    validate_stage_id(stage_id).map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    validate_invocation_id(invocation_id)
        .map_err(|error| StoreError::UnsafePath(error.to_string()))?;
    let path = invocation_dir(work_dir, stage_id, invocation_id);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => load_named(&path, OsStr::new(invocation_id), stage_id).map(|(state, _)| Some(state)),
    }
}

fn sort_units(units: &mut [Result<LoadedUnit, StoreError>]) {
    units.sort_by(|a, b| match (a, b) {
        (Ok((a, _)), Ok((b, _))) => b
            .manifest
            .created_at
            .cmp(&a.manifest.created_at)
            .then_with(|| b.manifest.invocation_id.cmp(&a.manifest.invocation_id)),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => Ordering::Equal,
    });
}

fn load_all(work_dir: &Path, stage_id: &str) -> Vec<Result<LoadedUnit, StoreError>> {
    if let Err(error) = validate_stage_id(stage_id) {
        return vec![Err(StoreError::UnsafePath(error.to_string()))];
    }
    let mut units: Vec<_> = list_invocations(&stage_dir(work_dir, stage_id))
        .into_iter()
        .map(|entry| entry.and_then(|entry| load_one(entry, stage_id)))
        .collect();
    sort_units(&mut units);
    units
}

pub fn stage_units(work_dir: &Path, stage_id: &str) -> Vec<Result<NativeUnitState, StoreError>> {
    load_all(work_dir, stage_id)
        .into_iter()
        .map(|unit| unit.map(|(state, _)| state))
        .collect()
}

pub fn unit_state(
    work_dir: &Path,
    stage_id: &str,
    unit_id: &str,
) -> Result<Option<NativeUnitState>, StoreError> {
    // Inspect every record before returning a match: corrupt siblings cannot be attributed.
    let units: Vec<_> = stage_units(work_dir, stage_id)
        .into_iter()
        .collect::<Result<_, _>>()?;
    Ok(units
        .into_iter()
        .find(|unit| unit.manifest.unit_id == unit_id))
}

pub fn stage_disposition(work_dir: &Path, stage_id: &str) -> NativeDisposition {
    let units = stage_units(work_dir, stage_id);
    if let Some(error) = units.iter().find_map(|unit| unit.as_ref().err()) {
        // Record errors carry their context; errors listing or validating the stage need it.
        return NativeDisposition::Unverified(match error {
            StoreError::Corrupt(detail) => detail.clone(),
            _ => format!("stage {stage_id}: {error}"),
        });
    }
    let units: Vec<_> = units.into_iter().filter_map(Result::ok).collect();
    if let Some(unit) = units
        .iter()
        .find(|unit| unit.ownership == Ownership::Unverified)
    {
        return NativeDisposition::Unverified(format!(
            "{}{UNPROVEN_OWNERSHIP}",
            unit.manifest.invocation_id
        ));
    }
    let mut active = Vec::new();
    for unit in units {
        if (unit.phase != Phase::Terminal || unit.ownership == Ownership::Held)
            && !active.contains(&unit.manifest.unit_id)
        {
            active.push(unit.manifest.unit_id);
        }
    }
    if active.is_empty() {
        NativeDisposition::None
    } else {
        NativeDisposition::Active(active)
    }
}

pub fn stage_threads(work_dir: &Path, stage_id: &str) -> Vec<NativeThread> {
    load_all(work_dir, stage_id)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|(unit, events)| {
            let thread_id = unit.thread_id?;
            let started_at = events.iter().rev().find_map(|record| match &record.event {
                NativeEvent::ThreadStarted { thread_id: id } if id == &thread_id => Some(record.at),
                _ => None,
            })?;
            Some(NativeThread {
                thread_id,
                unit_id: unit.manifest.unit_id,
                invocation_id: unit.manifest.invocation_id,
                requested_model: unit.manifest.model,
                requested_effort: unit.manifest.effort,
                started_at,
            })
        })
        .collect()
}

pub fn stages(work_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(work_dir.join("native-workers")) else {
        return Vec::new();
    };
    let mut stages: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if validate_stage_id(&name).is_ok() && entry.file_type().ok()?.is_dir() {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    stages.sort();
    stages
}

#[cfg(test)]
#[path = "index_tests_damage.rs"]
mod damage_tests;

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
