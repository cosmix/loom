//! Filesystem-backed [`EvidenceSource`]: classifies a bound worker from the
//! stage's lifecycle journal, and for a Codex companion still running, from
//! its live job record.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::subagent_lifecycle::{CodexExecution, LifecycleState, WorkerIdentity, WorkerOutcome};

use super::engine::EvidenceSource;
use super::model::{BoundWorker, WorkerKind};
use super::{native, stall};

/// Filesystem-backed lifecycle evidence reader.
pub struct LifecycleEvidence {
    /// Loom work directory containing lifecycle journals.
    pub work_dir: PathBuf,
    /// How long a worker may make no observable progress before it counts as
    /// hung. Only ever consulted for a worker the journal still calls active.
    pub stall_budget: Duration,
}

impl EvidenceSource for LifecycleEvidence {
    fn outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        self.poll(std::slice::from_ref(worker)).remove(0).0
    }

    fn unknown_settled(&self, worker: &BoundWorker) -> bool {
        // A direct forward's `unknown` record is reconcile's last word on it:
        // the receipt folded to `Unknown`, which absorbs every later
        // observation, so the watch would otherwise hold until its deadline.
        // An unreadable journal settles nothing: the forward may still run.
        self.poll(std::slice::from_ref(worker))[0].1
    }

    fn poll(&self, workers: &[BoundWorker]) -> Vec<(WorkerOutcome, bool)> {
        let index = crate::subagent_lifecycle::replay(&self.work_dir);
        workers
            .iter()
            .map(|worker| {
                if worker.native_invocation.is_some() {
                    return self.native_poll(worker);
                }
                let outcome = match &index {
                    Ok(index) => self.outcome_from_index(worker, index),
                    Err(error) => {
                        WorkerOutcome::Unknown(format!("lifecycle replay failed: {error:#}"))
                    }
                };
                let settled = matches!(outcome, WorkerOutcome::Unknown(_))
                    && index
                        .as_ref()
                        .is_ok_and(|index| unknown_settled(index, worker));
                (outcome, settled)
            })
            .collect()
    }
}

impl LifecycleEvidence {
    /// A native worker is read from its own record, never the lifecycle
    /// journal. Every native `Unknown` is final: nothing later changes it, so
    /// the wait ends with exit 5 instead of holding to its deadline.
    fn native_poll(&self, worker: &BoundWorker) -> (WorkerOutcome, bool) {
        let outcome = match native::state_of(&self.work_dir, worker) {
            Ok(state) => native::outcome_for(&state, chrono::Utc::now(), self.stall_budget),
            Err(reason) => WorkerOutcome::Unknown(reason),
        };
        let settled = matches!(outcome, WorkerOutcome::Unknown(_));
        (outcome, settled)
    }

    fn outcome_from_index(
        &self,
        worker: &BoundWorker,
        index: &crate::subagent_lifecycle::LifecycleIndex,
    ) -> WorkerOutcome {
        let outcome = match worker.worker.kind {
            WorkerKind::Claude => self.claude_from_index(worker, index),
            WorkerKind::Codex => self.codex_from_index(worker, index),
        };
        match outcome {
            WorkerOutcome::Active => stall::detect(worker, &self.work_dir, self.stall_budget)
                .unwrap_or(WorkerOutcome::Active),
            outcome => outcome,
        }
    }

    fn claude_from_index(
        &self,
        worker: &BoundWorker,
        index: &crate::subagent_lifecycle::LifecycleIndex,
    ) -> WorkerOutcome {
        let Some(identity) = worker.lifecycle_identity.as_ref() else {
            return WorkerOutcome::Unknown("worker has no lifecycle identity".into());
        };
        match identity {
            WorkerIdentity::ClaudeSubagent {
                stage_id,
                loom_session_id,
                parent_session_id,
                agent_id,
                transcript_path,
                ..
            } => {
                let outcome = index.claude_outcome(
                    stage_id,
                    loom_session_id,
                    parent_session_id,
                    agent_id,
                    transcript_path,
                );
                if matches!(
                    &outcome,
                    WorkerOutcome::Unknown(reason) if reason == "no matching lifecycle evidence"
                ) && missing_or_idle_claude_evidence(index, identity)
                {
                    WorkerOutcome::Active
                } else {
                    outcome
                }
            }
            WorkerIdentity::ClaudeTeammate { .. } => index.outcome(identity),
            WorkerIdentity::Codex { .. } => {
                WorkerOutcome::Unknown("Claude worker has a Codex lifecycle identity".into())
            }
        }
    }

    fn codex_from_index(
        &self,
        worker: &BoundWorker,
        index: &crate::subagent_lifecycle::LifecycleIndex,
    ) -> WorkerOutcome {
        let Some(identity) = worker.lifecycle_identity.as_ref() else {
            return WorkerOutcome::Unknown("worker has no lifecycle identity".into());
        };
        let Some(authority) = &worker.authority else {
            return WorkerOutcome::Unknown("missing Codex authority".into());
        };
        match identity {
            WorkerIdentity::Codex {
                execution: CodexExecution::Companion { .. },
                ..
            } => {
                let journal = index.outcome(identity);
                if terminal_outcome(&journal) || has_exact_terminal_record(index, identity) {
                    journal
                } else {
                    crate::codex_lifecycle::companion_outcome_with_progress(
                        &self.work_dir,
                        &authority.authorization(),
                        self.stall_budget,
                        SystemTime::now(),
                    )
                }
            }
            WorkerIdentity::Codex {
                execution: CodexExecution::Direct { .. },
                ..
            } => index.outcome(identity),
            _ => WorkerOutcome::Unknown("Codex worker has a non-Codex lifecycle identity".into()),
        }
    }
}

fn missing_or_idle_claude_evidence(
    index: &crate::subagent_lifecycle::LifecycleIndex,
    expected: &WorkerIdentity,
) -> bool {
    let WorkerIdentity::ClaudeSubagent {
        stage_id,
        loom_session_id,
        parent_session_id,
        agent_id,
        ..
    } = expected
    else {
        return false;
    };
    let records = match index.stage_records(stage_id) {
        Ok(records) => records,
        Err(_) => return false,
    };
    records.iter().all(|record| match &record.identity {
        WorkerIdentity::ClaudeSubagent { agent_id: seen, .. } => seen != agent_id,
        WorkerIdentity::ClaudeTeammate {
            stage_id: stage,
            loom_session_id: loom,
            parent_session_id: parent,
            teammate_name,
            ..
        } if teammate_name == agent_id => {
            stage == stage_id
                && loom == loom_session_id
                && parent == parent_session_id
                && index.outcome(&record.identity) == WorkerOutcome::Active
        }
        _ => true,
    })
}

fn has_exact_terminal_record(
    index: &crate::subagent_lifecycle::LifecycleIndex,
    expected: &WorkerIdentity,
) -> bool {
    has_exact_record(index, expected, true, |state| {
        matches!(
            state,
            LifecycleState::Completed
                | LifecycleState::Failed
                | LifecycleState::Cancelled
                | LifecycleState::Unknown
        )
    })
}

/// Whether the stage journal holds a record for exactly `expected` whose
/// state satisfies `wanted`. An unreadable journal answers `unreadable`: true
/// where the caller must stop trusting live progress, false where a match
/// would end a wait. A missing journal never matches.
fn has_exact_record(
    index: &crate::subagent_lifecycle::LifecycleIndex,
    expected: &WorkerIdentity,
    unreadable: bool,
    wanted: impl Fn(LifecycleState) -> bool,
) -> bool {
    let records = match index.stage_records(expected.stage_id()) {
        Ok(records) => records,
        Err(_) => return unreadable,
    };
    records
        .iter()
        .any(|record| &record.identity == expected && wanted(record.state))
}

fn unknown_settled(
    index: &crate::subagent_lifecycle::LifecycleIndex,
    worker: &BoundWorker,
) -> bool {
    let Some(identity) = &worker.lifecycle_identity else {
        return false;
    };
    worker.worker.kind == WorkerKind::Codex
        && matches!(
            identity,
            WorkerIdentity::Codex {
                execution: CodexExecution::Direct { .. },
                ..
            }
        )
        && has_exact_record(index, identity, false, |state| {
            state == LifecycleState::Unknown
        })
}

fn terminal_outcome(outcome: &WorkerOutcome) -> bool {
    matches!(
        outcome,
        WorkerOutcome::Succeeded | WorkerOutcome::Failed(_) | WorkerOutcome::Cancelled(_)
    )
}
