use std::time::Duration;

use crate::subagent_lifecycle::WorkerOutcome;

use super::lease::{deadline_state, BootClock, DeadlineState};
use super::model::{BootDeadline, BoundWorker, TerminalOutcome, WaitIdentity, WorkerKind};
use super::stall;

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

    /// Whether an [`WorkerOutcome::Unknown`] for `worker` is final: its
    /// lifecycle journal closed the execution as `unknown`, so no later poll
    /// can change the answer. Missing or inconclusive evidence is not final.
    fn unknown_settled(&self, _worker: &BoundWorker) -> bool {
        false
    }
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
        let settled: Vec<bool> = identity
            .workers
            .iter()
            .zip(&outcomes)
            .map(|(worker, outcome)| {
                matches!(outcome, WorkerOutcome::Unknown(_)) && evidence.unknown_settled(worker)
            })
            .collect();
        if let Some(result) = terminal_evidence(identity, &outcomes, &settled, polls) {
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

/// The first worker whose outcome `pick` accepts, with the reason it carries.
fn first_outcome<'a>(
    outcomes: &'a [WorkerOutcome],
    pick: impl Fn(usize, &'a WorkerOutcome) -> Option<&'a String>,
) -> Option<(usize, &'a String)> {
    outcomes
        .iter()
        .enumerate()
        .find_map(|(index, outcome)| pick(index, outcome).map(|reason| (index, reason)))
}

/// `settled[i]` says worker `i`'s `Unknown` outcome is final (see
/// [`EvidenceSource::unknown_settled`]); it ends the wait like a terminal
/// verdict, after any failure or cancellation. An unsettled `Unknown` holds
/// the wait until the deadline.
fn terminal_evidence(
    identity: &WaitIdentity,
    outcomes: &[WorkerOutcome],
    settled: &[bool],
    polls: u64,
) -> Option<EngineResult> {
    let name = |index: usize| worker_name(&identity.workers[index]);
    if let Some((index, reason)) = first_outcome(outcomes, |_, outcome| match outcome {
        WorkerOutcome::Failed(reason) => Some(reason),
        _ => None,
    }) {
        let detail = format!("{} failed: {reason}", name(index));
        return Some(result(TerminalOutcome::Failed, detail, polls));
    }
    if let Some((index, reason)) = first_outcome(outcomes, |_, outcome| match outcome {
        WorkerOutcome::Cancelled(reason) => Some(reason),
        _ => None,
    }) {
        let detail = format!("{} cancelled: {reason}", name(index));
        return Some(result(TerminalOutcome::Cancelled, detail, polls));
    }
    if let Some((index, reason)) = first_outcome(outcomes, |index, outcome| match outcome {
        WorkerOutcome::Unknown(reason) if settled.get(index).copied().unwrap_or(false) => {
            Some(reason)
        }
        _ => None,
    }) {
        let detail = format!("{} unknown: {reason}", name(index));
        return Some(result(TerminalOutcome::Unknown, detail, polls));
    }
    if let Some(detail) = stall::stalled_detail(identity, outcomes) {
        return Some(result(TerminalOutcome::Stalled, detail, polls));
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

pub(super) fn worker_name(worker: &BoundWorker) -> String {
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
