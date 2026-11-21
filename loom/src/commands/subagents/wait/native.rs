//! Native Codex workers in the bounded wait.
//!
//! A `codex:<unit>` selector binds to the stage's native record when one
//! exists for the unit; every poll then reads that record directly. The
//! lifecycle journal is never consulted for such a worker, so a corrupt journal
//! cannot turn a native result into `Unknown`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};

use crate::native_worker::index::{unit_snapshot, unit_state};
use crate::native_worker::model::{
    EventRecord, NativeEvent, NativeUnitState, Outcome, Ownership, Phase,
};
use crate::native_worker::store::invocation_dir;
use crate::subagent_lifecycle::WorkerOutcome;

use super::identity::IdentityScope;
use super::model::{BoundWorker, EvidenceReference, WorkerKind, WorkerSpec};

/// Evidence source name recorded for a native worker.
const NATIVE_SOURCE: &str = "native_worker";

/// Map one folded native record onto the wait's outcome vocabulary.
pub(super) fn outcome_for(
    state: &NativeUnitState,
    now: DateTime<Utc>,
    stall_budget: Duration,
) -> WorkerOutcome {
    if state.ownership == Ownership::Unverified {
        return WorkerOutcome::Unknown(
            "native worker processes could not be proven stopped".into(),
        );
    }
    if state.phase != Phase::Terminal {
        let budget = chrono::Duration::from_std(stall_budget).unwrap_or(chrono::Duration::MAX);
        let quiet = now - state.last_progress_at;
        return if quiet > budget {
            WorkerOutcome::Stalled(format!(
                "native worker {} made no progress for {}s (budget {}s)",
                state.manifest.unit_id,
                quiet.num_seconds(),
                stall_budget.as_secs()
            ))
        } else {
            WorkerOutcome::Active
        };
    }
    if state.ownership == Ownership::Held {
        return WorkerOutcome::Active;
    }
    let reason = || {
        state
            .outcome_reason
            .clone()
            .unwrap_or_else(|| "no reason recorded".to_owned())
    };
    match state.outcome {
        Some(Outcome::Succeeded) => WorkerOutcome::Succeeded,
        Some(Outcome::Cancelled) => WorkerOutcome::Cancelled(reason()),
        Some(Outcome::Failed | Outcome::TimedOut) => WorkerOutcome::Failed(reason()),
        Some(Outcome::Unknown) => WorkerOutcome::Unknown(reason()),
        None => WorkerOutcome::Unknown("terminal record has no outcome".into()),
    }
}

/// `<stage_id>/<invocation_id>`: how a bound worker names its record.
fn locator(stage_id: &str, invocation_id: &str) -> String {
    format!("{stage_id}/{invocation_id}")
}

/// Re-read the record a native worker was bound to.
///
/// `unit_state` returns the newest invocation of the unit, so a restart shows
/// up as a different invocation id and ends the wait instead of silently
/// following the new run.
pub(super) fn state_of(work_dir: &Path, worker: &BoundWorker) -> Result<NativeUnitState, String> {
    snapshot_of(work_dir, worker).map(|(state, _)| state)
}

/// [`state_of`] with the events the state was folded from, from one index read.
pub(super) fn snapshot_of(
    work_dir: &Path,
    worker: &BoundWorker,
) -> Result<(NativeUnitState, Vec<EventRecord>), String> {
    let Some((stage, invocation)) = worker
        .native_invocation
        .as_deref()
        .and_then(|locator| locator.split_once('/'))
    else {
        return Err("native worker has no bound invocation".into());
    };
    let unit = &worker.worker.id;
    let (state, events) = unit_snapshot(work_dir, stage, unit)
        .map_err(|error| format!("native worker record is unreadable: {error:?}"))?
        .ok_or_else(|| "native worker record is missing".to_owned())?;
    if state.manifest.invocation_id != invocation {
        return Err(format!(
            "unit '{unit}' was restarted as {}; watch it again",
            state.manifest.invocation_id
        ));
    }
    Ok((state, events))
}

/// Progress of a native worker the wait still considers `Active`.
pub(super) struct ActiveProgress {
    /// Seconds since the newest provider event; `None` before the first one.
    pub(super) age_secs: Option<u64>,
}

/// The newest thread, turn or progress event. Admission, boundary and control
/// events are loom's own bookkeeping, so they never count as provider progress.
fn provider_progress_at(events: &[EventRecord]) -> Option<DateTime<Utc>> {
    events
        .iter()
        .filter(|record| {
            matches!(
                record.event,
                NativeEvent::ThreadStarted { .. }
                    | NativeEvent::TurnStarted { .. }
                    | NativeEvent::Progress
            )
        })
        .map(|record| record.at)
        .max()
}

/// Whole seconds from `at` to `now`; a time in the future reads as 0.
fn age_secs(at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    u64::try_from((now - at).num_seconds()).unwrap_or(0)
}

/// Read the worker's record once and report its provider progress when its
/// outcome is `Active`; any other state or unreadable record yields `None`.
pub(super) fn active_progress(
    work_dir: &Path,
    worker: &BoundWorker,
    now: DateTime<Utc>,
    stall_budget: Duration,
) -> Option<ActiveProgress> {
    let (state, events) = snapshot_of(work_dir, worker).ok()?;
    (outcome_for(&state, now, stall_budget) == WorkerOutcome::Active).then(|| ActiveProgress {
        age_secs: provider_progress_at(&events).map(|at| age_secs(at, now)),
    })
}

/// The codex units of a wait set that have a native record. Worker ids are
/// unique across kinds in one set, so an id key identifies its codex spec.
pub(super) struct NativeSet {
    units: BTreeMap<String, NativeUnitState>,
}

impl NativeSet {
    /// A corrupt record is an error, never a fallback to the legacy binding.
    pub(super) fn load(scope: &IdentityScope, specs: &[WorkerSpec]) -> Result<Self> {
        let mut units = BTreeMap::new();
        for spec in specs.iter().filter(|spec| spec.kind == WorkerKind::Codex) {
            match unit_state(&scope.work_dir, &scope.stage_id, &spec.id) {
                Ok(Some(state)) => {
                    units.insert(spec.id.clone(), state);
                }
                Ok(None) => {}
                Err(error) => bail!(
                    "native worker record for unit '{}' is unreadable: {error:?}",
                    spec.id
                ),
            }
        }
        Ok(Self { units })
    }

    /// The specs that bind through the authorization row or the Claude parent.
    pub(super) fn legacy(&self, specs: &[WorkerSpec]) -> Vec<WorkerSpec> {
        specs
            .iter()
            .filter(|spec| !self.units.contains_key(&spec.id))
            .cloned()
            .collect()
    }

    pub(super) fn bind(
        &self,
        scope: &IdentityScope,
        spec: &WorkerSpec,
    ) -> Result<Option<BoundWorker>> {
        let Some(state) = self.units.get(&spec.id) else {
            return Ok(None);
        };
        let manifest = &state.manifest;
        ensure!(
            manifest.workspace_root == scope.canonical_worktree,
            "native worker record names another worktree"
        );
        Ok(Some(BoundWorker {
            worker: spec.clone(),
            lifecycle_identity: None,
            native_invocation: Some(locator(&scope.stage_id, &manifest.invocation_id)),
            authority: None,
            evidence: vec![EvidenceReference {
                source: NATIVE_SOURCE.into(),
                path: invocation_dir(&scope.work_dir, &scope.stage_id, &manifest.invocation_id),
                identity: manifest.invocation_id.clone(),
            }],
        }))
    }

    /// The parent of a set with no Claude or legacy Codex worker: native
    /// workers have no Claude parent, so the Loom session stands in.
    pub(super) fn parent_when_all_native(
        scope: &IdentityScope,
        requested: Option<&str>,
    ) -> Result<String> {
        ensure!(
            requested.is_none(),
            "--session names a Claude parent, but every worker is a native Codex worker"
        );
        Ok(scope.loom_session_id.clone())
    }
}
