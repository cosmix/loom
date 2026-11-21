mod codex_binding;
mod engine;
mod evidence;
mod identity;
mod lease;
mod model;
mod native;
mod output;
mod parent_scan;
mod stall;

#[cfg(test)]
mod direct_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod lease_boot_tests;
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod native_contract_tests;
#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod progress_contract_tests;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};

use engine::{wait_for_workers, EngineResult, EvidenceSource, ThreadSleeper, POLL_INTERVAL};
use evidence::LifecycleEvidence;
use lease::{
    acquire, finish, prune_results, Acquired, LeaseDir, SystemBootClock, SystemOwnerProbe,
};
use model::{exit_code, BoundWorker, EventOutcome, WaitLease, WorkerKind, EXIT_UNKNOWN};

use crate::native_worker::ids::PROGRESS_EVENT_INTERVAL_SECS;
use crate::subagent_lifecycle::WorkerOutcome;

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
    // The JSON stream stays exactly the initial and terminal records.
    let reporting = ProgressReporter::new(&evidence);
    let source: &dyn EvidenceSource = if json { &evidence } else { &reporting };
    let EngineResult {
        outcome, detail, ..
    } = wait_for_workers(
        &lease.identity,
        &lease.deadline,
        clock,
        &sleeper,
        source,
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

/// Reports on a human-readable wait while it runs: delegates every evidence
/// read to `inner` and, after each poll that continues, writes progress lines
/// for the active native Codex workers.
struct ProgressReporter<'a> {
    inner: &'a LifecycleEvidence,
    last_emit: Cell<Option<DateTime<Utc>>>,
}

impl<'a> ProgressReporter<'a> {
    fn new(inner: &'a LifecycleEvidence) -> Self {
        Self {
            inner,
            last_emit: Cell::new(None),
        }
    }
}

impl EvidenceSource for ProgressReporter<'_> {
    fn outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        self.inner.outcome(worker)
    }

    fn unknown_settled(&self, worker: &BoundWorker) -> bool {
        self.inner.unknown_settled(worker)
    }

    fn poll(&self, workers: &[BoundWorker]) -> Vec<(WorkerOutcome, bool)> {
        self.inner.poll(workers)
    }

    fn continuing(&self, workers: &[BoundWorker]) {
        let mut last_emit = self.last_emit.get();
        // Progress lines are a diagnostic: a closed stdout must not end the wait,
        // and the terminal record reports it if it persists.
        let _ = emit_active_native_progress(
            self.inner,
            workers,
            Utc::now(),
            &mut last_emit,
            &mut io::stdout().lock(),
        );
        self.last_emit.set(last_emit);
    }
}

/// Write one line per active native Codex worker saying how long ago its
/// provider last made progress. Writes on the first call, then at most once
/// per [`PROGRESS_EVENT_INTERVAL_SECS`]; a call inside the interval reads and
/// writes nothing. Claude workers and legacy Codex jobs get no line.
fn emit_active_native_progress(
    evidence: &LifecycleEvidence,
    workers: &[BoundWorker],
    now: DateTime<Utc>,
    last_emit: &mut Option<DateTime<Utc>>,
    out: &mut dyn Write,
) -> io::Result<()> {
    let interval = chrono::Duration::seconds(PROGRESS_EVENT_INTERVAL_SECS as i64);
    if last_emit.is_some_and(|last| now - last < interval) {
        return Ok(());
    }
    let mut natives = workers
        .iter()
        .filter(|worker| {
            worker.worker.kind == WorkerKind::Codex && worker.native_invocation.is_some()
        })
        .peekable();
    if natives.peek().is_none() {
        return Ok(());
    }
    *last_emit = Some(now);
    for worker in natives {
        let progress =
            native::active_progress(&evidence.work_dir, worker, now, evidence.stall_budget);
        if let Some(progress) = progress {
            let name = engine::worker_name(worker);
            writeln!(out, "{}", output::progress_line(&name, progress.age_secs))?;
        }
    }
    out.flush()
}
