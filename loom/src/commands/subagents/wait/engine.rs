use std::path::PathBuf;
use std::time::Duration;

use crate::subagent_lifecycle::{CodexExecution, LifecycleState, WorkerIdentity, WorkerOutcome};

use super::lease::{deadline_state, BootClock, DeadlineState};
use super::model::{BootDeadline, BoundWorker, TerminalOutcome, WaitIdentity, WorkerKind};

/// Default interval between fresh lifecycle evidence reads.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Blocking sleep boundary used by the wait loop.
pub trait Sleeper {
    /// Block for `duration` before the next evidence poll.
    fn sleep(&self, duration: Duration);
}

/// Production sleeper backed by the current thread.
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Source of a worker's latest authoritative lifecycle outcome.
pub trait EvidenceSource {
    /// Read and classify fresh evidence for `worker`.
    fn outcome(&self, worker: &BoundWorker) -> WorkerOutcome;
}

/// Filesystem-backed lifecycle evidence reader.
pub struct LifecycleEvidence {
    /// Loom work directory containing lifecycle journals.
    pub work_dir: PathBuf,
}

impl EvidenceSource for LifecycleEvidence {
    fn outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        match worker.worker.kind {
            WorkerKind::Claude => self.claude_outcome(worker),
            WorkerKind::Codex => self.codex_outcome(worker),
        }
    }
}

impl LifecycleEvidence {
    fn claude_outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        let index = match crate::subagent_lifecycle::replay(&self.work_dir) {
            Ok(index) => index,
            Err(error) => return replay_error(error),
        };
        match &worker.lifecycle_identity {
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
                ) && missing_or_idle_claude_evidence(
                    &self.work_dir,
                    &index,
                    &worker.lifecycle_identity,
                ) {
                    WorkerOutcome::Active
                } else {
                    outcome
                }
            }
            WorkerIdentity::ClaudeTeammate { .. } => index.outcome(&worker.lifecycle_identity),
            WorkerIdentity::Codex { .. } => {
                WorkerOutcome::Unknown("Claude worker has a Codex lifecycle identity".into())
            }
        }
    }

    fn codex_outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        let Some(authority) = &worker.authority else {
            return WorkerOutcome::Unknown("missing Codex authority".into());
        };
        let index = match crate::subagent_lifecycle::replay(&self.work_dir) {
            Ok(index) => index,
            Err(error) => return replay_error(error),
        };
        match &worker.lifecycle_identity {
            WorkerIdentity::Codex {
                execution: CodexExecution::Companion { .. },
                ..
            } => {
                let journal = index.outcome(&worker.lifecycle_identity);
                if terminal_outcome(&journal)
                    || has_exact_terminal_record(&self.work_dir, &worker.lifecycle_identity)
                {
                    journal
                } else {
                    crate::codex_lifecycle::companion_outcome(
                        &self.work_dir,
                        &authority.authorization(),
                    )
                }
            }
            WorkerIdentity::Codex {
                execution: CodexExecution::Direct { .. },
                ..
            } => index.outcome(&worker.lifecycle_identity),
            _ => WorkerOutcome::Unknown("Codex worker has a non-Codex lifecycle identity".into()),
        }
    }
}

fn missing_or_idle_claude_evidence(
    work_dir: &std::path::Path,
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
    let records = match lifecycle_records(work_dir, stage_id) {
        Ok(records) => records,
        Err(error) => return error.kind() == std::io::ErrorKind::NotFound,
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

fn has_exact_terminal_record(work_dir: &std::path::Path, expected: &WorkerIdentity) -> bool {
    let records = match lifecycle_records(work_dir, expected.stage_id()) {
        Ok(records) => records,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    records.iter().any(|record| {
        &record.identity == expected
            && matches!(
                record.state,
                LifecycleState::Completed
                    | LifecycleState::Failed
                    | LifecycleState::Cancelled
                    | LifecycleState::Unknown
            )
    })
}

fn lifecycle_records(
    work_dir: &std::path::Path,
    stage_id: &str,
) -> std::io::Result<Vec<crate::subagent_lifecycle::LifecycleRecord>> {
    let path = work_dir
        .join("subagents")
        .join(stage_id)
        .join("lifecycle.jsonl");
    crate::codex_lifecycle::read_lifecycle_records(&path).map_err(|error| {
        error
            .downcast::<std::io::Error>()
            .unwrap_or_else(|error| std::io::Error::other(error.to_string()))
    })
}

fn terminal_outcome(outcome: &WorkerOutcome) -> bool {
    matches!(
        outcome,
        WorkerOutcome::Succeeded | WorkerOutcome::Failed(_) | WorkerOutcome::Cancelled(_)
    )
}

fn replay_error(error: anyhow::Error) -> WorkerOutcome {
    WorkerOutcome::Unknown(format!("lifecycle replay failed: {error}"))
}

/// Terminal result returned by the bounded wait engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineResult {
    /// Final classification of the wait.
    pub outcome: TerminalOutcome,
    /// Human-readable evidence or deadline detail.
    pub detail: Option<String>,
    /// Number of sleeps completed before termination.
    pub polls: u64,
}

/// Poll fresh evidence until all workers terminate or the boot deadline ends the wait.
pub fn wait_for_workers(
    identity: &WaitIdentity,
    deadline: &BootDeadline,
    clock: &dyn BootClock,
    sleeper: &dyn Sleeper,
    evidence: &dyn EvidenceSource,
    interval: Duration,
) -> EngineResult {
    let mut polls = 0_u64;
    loop {
        let outcomes: Vec<_> = identity
            .workers
            .iter()
            .map(|worker| evidence.outcome(worker))
            .collect();
        if let Some(result) = terminal_evidence(identity, &outcomes, polls) {
            return result;
        }
        match deadline_state(deadline, clock) {
            Ok(DeadlineState::Remaining(remaining)) => {
                sleeper.sleep(interval.min(remaining));
                polls = polls.saturating_add(1);
            }
            Ok(DeadlineState::Expired) => return expired(identity, &outcomes, polls),
            Ok(DeadlineState::BootChanged) => {
                return result(
                    TerminalOutcome::Interrupted,
                    "system boot changed while waiting",
                    polls,
                );
            }
            Err(error) => {
                return result(
                    TerminalOutcome::Interrupted,
                    format!("deadline check failed: {error}"),
                    polls,
                );
            }
        }
    }
}

fn terminal_evidence(
    identity: &WaitIdentity,
    outcomes: &[WorkerOutcome],
    polls: u64,
) -> Option<EngineResult> {
    if let Some((index, reason)) =
        outcomes
            .iter()
            .enumerate()
            .find_map(|(index, outcome)| match outcome {
                WorkerOutcome::Failed(reason) => Some((index, reason)),
                _ => None,
            })
    {
        let detail = format!("{} failed: {reason}", worker_name(&identity.workers[index]));
        return Some(result(TerminalOutcome::Failed, detail, polls));
    }
    if let Some((index, reason)) =
        outcomes
            .iter()
            .enumerate()
            .find_map(|(index, outcome)| match outcome {
                WorkerOutcome::Cancelled(reason) => Some((index, reason)),
                _ => None,
            })
    {
        let detail = format!(
            "{} cancelled: {reason}",
            worker_name(&identity.workers[index])
        );
        return Some(result(TerminalOutcome::Cancelled, detail, polls));
    }
    outcomes
        .iter()
        .all(|outcome| matches!(outcome, WorkerOutcome::Succeeded))
        .then(|| {
            result(
                TerminalOutcome::Succeeded,
                format!(
                    "all {} bound workers have fresh correlated success evidence",
                    outcomes.len()
                ),
                polls,
            )
        })
}

fn expired(identity: &WaitIdentity, outcomes: &[WorkerOutcome], polls: u64) -> EngineResult {
    let unknown: Vec<_> = outcomes
        .iter()
        .enumerate()
        .filter_map(|(index, outcome)| match outcome {
            WorkerOutcome::Unknown(reason) => Some(format!(
                "{} ({reason})",
                worker_name(&identity.workers[index])
            )),
            _ => None,
        })
        .collect();
    if !unknown.is_empty() {
        return result(
            TerminalOutcome::Unknown,
            format!(
                "unknown worker evidence at deadline: {}",
                unknown.join(", ")
            ),
            polls,
        );
    }
    let active = named_workers(identity, outcomes, |outcome| {
        matches!(outcome, WorkerOutcome::Active)
    });
    result(
        TerminalOutcome::TimedOut,
        format!(
            "deadline expired with active workers: {}; deadline expiry is not proof that a worker died",
            active.join(", ")
        ),
        polls,
    )
}

fn named_workers(
    identity: &WaitIdentity,
    outcomes: &[WorkerOutcome],
    predicate: impl Fn(&WorkerOutcome) -> bool,
) -> Vec<String> {
    identity
        .workers
        .iter()
        .zip(outcomes)
        .filter(|(_, outcome)| predicate(outcome))
        .map(|(worker, _)| worker_name(worker))
        .collect()
}

fn worker_name(worker: &BoundWorker) -> String {
    let kind = match worker.worker.kind {
        WorkerKind::Claude => "claude",
        WorkerKind::Codex => "codex",
    };
    format!("{kind}:{}", worker.worker.id)
}

fn result(outcome: TerminalOutcome, detail: impl Into<String>, polls: u64) -> EngineResult {
    EngineResult {
        outcome,
        detail: Some(detail.into()),
        polls,
    }
}
