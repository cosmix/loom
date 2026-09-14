mod codex_binding;
mod engine;
mod identity;
mod lease;
mod model;
mod output;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};

use engine::{wait_for_workers, EngineResult, LifecycleEvidence, ThreadSleeper, POLL_INTERVAL};
use lease::{
    acquire, finish, prune_results, Acquired, LeaseDir, SystemBootClock, SystemOwnerProbe,
};
use model::{exit_code, EventOutcome, WaitLease, EXIT_UNKNOWN};

pub struct WatchRequest {
    pub workers: Vec<String>,
    pub session: Option<String>,
    pub timeout_secs: u64,
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
        Acquired::Owner(lease) => own_wait(dir, lease, work_dir, request.json, &clock),
    }
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
    let evidence = LifecycleEvidence { work_dir };
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
