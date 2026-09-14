mod codex_binding;
mod engine;
mod identity;
mod lease;
mod model;
mod output;
mod stall;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};

use engine::{wait_for_workers, EngineResult, LifecycleEvidence, ThreadSleeper, POLL_INTERVAL};
use lease::{
    acquire, finish, prune_results, Acquired, LeaseDir, SystemBootClock, SystemOwnerProbe,
};
use model::{exit_code, EventOutcome, WaitLease, EXIT_UNKNOWN};

/// Stall budget used when neither the caller nor the stage names one.
///
/// Deliberately not [`crate::models::stage::Stage::effective_subagent_timeout_secs`],
/// whose 300s fallback is the session heartbeat budget: a thinking model's turn
/// routinely exceeds it, and a false stall would end the wait on a live worker.
const DEFAULT_STALL_SECS: u64 = 600;

pub struct WatchRequest {
    pub workers: Vec<String>,
    pub session: Option<String>,
    pub timeout_secs: u64,
    pub stall_secs: Option<u64>,
    pub json: bool,
    pub legacy_dir: Option<PathBuf>,
}

pub fn run(request: WatchRequest) -> Result<()> {
    reject_legacy_form(&request)?;
    let (identity, work_dir) = match identity::resolve_from_environment_with_work_dir(
        &request.workers,
        request.session.as_deref(),
    ) {
        Ok(bound) => bound,
        Err(error) => {
            output::emit_unknown(&error.to_string(), request.json)?;
            std::process::exit(EXIT_UNKNOWN);
        }
    };

    let clock = SystemBootClock;
    let probe = SystemOwnerProbe;
    let dir = LeaseDir::open(&std::env::temp_dir(), &identity)?;
    prune_results(&dir, &clock)?;
    let acquired = acquire(
        &dir,
        &identity,
        Duration::from_secs(request.timeout_secs),
        env!("LOOM_COMMIT"),
        &clock,
        &probe,
    )?;
    match acquired {
        Acquired::AlreadyWaiting(lease) => {
            existing_wait(lease, EventOutcome::AlreadyWaiting, request.json)
        }
        Acquired::Busy(lease) => existing_wait(lease, EventOutcome::Busy, request.json),
        Acquired::Owner(lease) => own_wait(
            dir,
            lease,
            work_dir,
            request.stall_secs,
            request.json,
            &clock,
        ),
    }
}

/// Resolve the stall budget: the explicit flag, else the stage's own subagent
/// timeout, else [`DEFAULT_STALL_SECS`].
fn stall_budget(stall_secs: Option<u64>, work_dir: &Path, stage_id: &str) -> Duration {
    let secs = stall_secs.unwrap_or_else(|| {
        crate::verify::load_stage(stage_id, work_dir)
            .ok()
            .and_then(|stage| stage.subagent_timeout_secs)
            .unwrap_or(DEFAULT_STALL_SECS)
    });
    Duration::from_secs(secs)
}

fn reject_legacy_form(request: &WatchRequest) -> Result<()> {
    if request.legacy_dir.is_some() || request.workers.is_empty() {
        bail!(
            "watch no longer polls a transcript directory; run `loom subagents watch --worker \
             claude:<agent-id> --worker codex:<unit-id> --timeout 3600`, and use `loom subagents \
             list` or `loom subagents harvest` for a one-shot diagnostic"
        );
    }
    Ok(())
}

fn existing_wait(lease: WaitLease, outcome: EventOutcome, json: bool) -> Result<()> {
    let workers = output::bound_workers(&lease);
    let detail = match &outcome {
        EventOutcome::AlreadyWaiting => format!(
            "existing wait {} already owns workers [{}]",
            lease.wait_id, workers
        ),
        EventOutcome::Busy => format!(
            "existing wait {} is bound to workers [{}]",
            lease.wait_id, workers
        ),
        _ => unreachable!("existing wait requires an ownership-conflict outcome"),
    };
    let event = output::event(&lease, outcome, Some(detail));
    output::emit(&event, json)?;
    std::process::exit(exit_code(&event.outcome));
}

fn own_wait(
    dir: LeaseDir,
    lease: WaitLease,
    work_dir: PathBuf,
    stall_secs: Option<u64>,
    json: bool,
    clock: &SystemBootClock,
) -> Result<()> {
    let initial = output::event(
        &lease,
        EventOutcome::Waiting,
        Some("bounded wait started with fresh lifecycle evidence reads".into()),
    );
    output::emit(&initial, json)?;

    let sleeper = ThreadSleeper;
    let stall_budget = stall_budget(stall_secs, &work_dir, &lease.identity.stage_id);
    let evidence = LifecycleEvidence {
        work_dir,
        stall_budget,
    };
    let EngineResult {
        outcome, detail, ..
    } = wait_for_workers(
        &lease.identity,
        &lease.deadline,
        clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );
    let event_outcome = output::terminal_outcome(&outcome);
    let code = exit_code(&event_outcome);
    finish(&dir, &lease, outcome, clock)?;
    let terminal = output::event(&lease, event_outcome, detail);
    output::emit(&terminal, json)?;
    match code {
        0 => Ok(()),
        code => std::process::exit(code),
    }
}
