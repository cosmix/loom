//! `loom hook reconcile-graph` — the A.12/A.22 debounced background self-heal.
//!
//! The prompt hook's own latency budget (a few seconds, well inside the shell
//! wrapper's 5s ceiling) must never pay for rebuilding a source graph, but a
//! stale or [`ContextPack::degraded`] pack (A.11) still deserves fixing —
//! just not on the request that noticed it. [`spawn_if_needed`] launches this
//! module's own subcommand, [`reconcile_graph`], detached and unawaited, so
//! the *next* retrieval benefits instead. See
//! `doc/PROPOSAL-retrieval-precision.md` §A.12/§A.22 and §P3 recommendation
//! 12 for the design.
//!
//! ## Lease and backoff
//!
//! See the `lock` submodule for the `reconcile.lock` line's encoding and the
//! full Spawn/Skip policy table. In short: the lock is a lease. A hook that
//! decides to spawn claims it, the reconciler re-stamps it with its own pid and
//! keeps that pid through every pass, and a request that arrives while it runs
//! only sets `pending`, which earns exactly one more pass. A run that does not
//! end `Current` counts a failure, and each failure doubles the throttle on
//! the next attempt. `loom hook reconcile-graph --cancel` stops the holder.
//!
//! ## Scope resolution
//!
//! `HookTarget` is shared with the prompt and pre-compact delegates, so the
//! reader and background writer cannot derive different overlay addresses.
//! [`ensure_snapshot`] owns both the stage-overlay and local-current rebuild
//! policies; this module only selects the policy carried by that target.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::context::config::RetrievalConfig;
use crate::context::freshness::{Freshness, GraphState};
use crate::context::graph_store::GraphStore;
use crate::context::refresh::{ensure_snapshot, SnapshotOutcome};
use crate::context::schema::ContextPack;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;

use super::target::{non_empty_env, HookTarget};

mod cancel;
mod lock;
use lock::{
    begin_run, decide, end_pass, held_by, mark_pending, read_lock, rebind_claim,
    reconcile_lock_path, release_failed, unix_now, update_lock, LockDecision, PassEnd,
};

/// The `loom hook reconcile-graph` subcommand body: a best-effort, one-shot
/// reconcile of the source graph for whatever scope
/// `HookTarget::from_environment` resolves, or with `cancel` a stop of the
/// running one.
///
/// Always exits `Ok(())` and prints nothing: this is an internal maintenance
/// entry point [`spawn_if_needed`] launches detached from a hook, so nothing
/// here may ever surface as a session-visible error or stray output. Every
/// failure — no resolvable state directory, a git failure, a graph-store I/O error —
/// is logged at `tracing::debug` and swallowed; a failed reconcile is counted
/// in the lock, not reported.
pub fn reconcile_graph(cancel: bool) -> Result<()> {
    let result = if cancel {
        try_cancel()
    } else {
        try_reconcile()
    };
    if let Err(error) = result {
        tracing::debug!(%error, "reconcile-graph: best-effort run did not complete");
    }
    Ok(())
}

/// The target and cache store this invocation acts on, or `None` when no
/// state directory is resolvable from the environment or cwd at all — nothing
/// to act on, and not a failure: a bare checkout with no loom project is a
/// legitimate place for this to be invoked from.
fn resolve_store() -> Result<Option<(HookTarget, ContextStore)>> {
    let Some(target) = HookTarget::from_environment().filter(HookTarget::exists) else {
        return Ok(None);
    };
    let work_dir = WorkDir::new(&target.work_dir)?;
    let store = ContextStore::open(&work_dir)?;
    Ok(Some((target, store)))
}

/// The fallible half of [`reconcile_graph`]: resolve scope, take over the
/// lease, and run passes until no request is pending.
fn try_reconcile() -> Result<()> {
    let Some((target, store)) = resolve_store()? else {
        return Ok(());
    };
    let lock_path = reconcile_lock_path(&store);
    let config = RetrievalConfig::load(&target.project_root);
    let claim = std::env::var("LOOM_RECONCILE_CLAIM")
        .ok()
        .map(|value| value.parse::<u128>())
        .transpose()?;
    run_passes(&lock_path, claim, &config, || {
        reconcile(&target, &store).state() != GraphState::Current
    });
    Ok(())
}

/// `--cancel`: stop the lease holder, if the lock names a live reconciler.
fn try_cancel() -> Result<()> {
    if let Some((_, store)) = resolve_store()? {
        cancel::cancel_holder(&reconcile_lock_path(&store), unix_now());
    }
    Ok(())
}

/// The holder protocol. A spawned child takes over only its parent's exact
/// claim; a direct invocation first competes through the normal policy.
/// After every pass, `lock::end_pass` either keeps the lease
/// (a request queued meanwhile: one more pass) or releases it with the run's
/// failure count, and a holder whose line another pid took over stops without
/// writing. `pass` returns whether the pass failed.
fn run_passes(
    lock_path: &Path,
    claim: Option<u128>,
    config: &RetrievalConfig,
    mut pass: impl FnMut() -> bool,
) {
    let pid = std::process::id();
    let acquired_claim = match claim {
        Some(claim) if begin_run(lock_path, pid, claim) => Some(claim),
        Some(_) => None,
        None => {
            let now = unix_now();
            (try_claim(
                lock_path,
                now,
                config.reconcile_debounce_secs,
                config.reconcile_stale_lock_secs,
                crate::process::is_process_alive,
                pid,
            ) == Claim::Won)
                .then(|| read_lock(lock_path).filter(|held| held.pid == pid && held.epoch == now))
                .flatten()
                .map(|held| held.claim)
        }
    };
    let Some(acquired_claim) = acquired_claim else {
        return;
    };
    loop {
        let failed = pass();
        if end_pass(lock_path, unix_now(), pid, acquired_claim, failed) != PassEnd::Again {
            break;
        }
    }
}

/// Reconcile exactly the graph scope resolved for the other hook delegates.
fn reconcile(target: &HookTarget, store: &ContextStore) -> SnapshotOutcome {
    let graph_store = GraphStore::new(store.root(), &target.work_dir);
    ensure_snapshot(
        store,
        &graph_store,
        &target.project_root,
        target.snapshot_policy(),
    )
}

/// Whether a pack's source graph warrants a background rebuild. The one
/// decision behind [`spawn_if_needed`]:
///
/// | `freshness.state()` | `degraded` | result |
/// | ------------------- | ---------- | ------ |
/// | `Stale`             | any        | true   |
/// | `Current`           | `Some`     | true   |
/// | `Current`           | `None`     | false  |
/// | `NeverBuilt`        | any        | false  |
/// | `Unavailable`       | any        | false  |
///
/// A never-built graph is built only by explicit commands (`loom init`,
/// `loom run`, `loom map`, `loom knowledge sync`), never by a prompt hook.
pub fn wants_rebuild(freshness: &Freshness, degraded: Option<&str>) -> bool {
    match freshness.state() {
        GraphState::Stale => true,
        GraphState::Current => degraded.is_some(),
        GraphState::NeverBuilt | GraphState::Unavailable => false,
    }
}

/// Spawn a detached `loom hook reconcile-graph` when [`wants_rebuild`] says
/// `pack`'s source graph needs one, throttled through `reconcile_lock_path`
/// so a burst of hook invocations spawns at most one reconcile at a time.
///
/// Fire-and-forget by contract: never waits on the child, never fails, never
/// prints — the hook's own latency budget must stay unaffected by however
/// long a background reconcile takes.
pub fn spawn_if_needed(pack: &ContextPack, project_root: &Path) {
    if !wants_rebuild(&pack.semantic_freshness, pack.degraded.as_deref()) {
        return;
    }
    if let Err(error) = try_spawn(project_root, unix_now()) {
        tracing::debug!(%error, "reconcile-graph: could not spawn a background reconcile");
    }
}

/// A full-tree rebuild needs an explicit `LOOM_WORK_DIR` or an existing cache.
/// An inferred checkout without a cache can come from an unrelated cwd.
fn allowed_to_spawn(store: &ContextStore) -> bool {
    non_empty_env("LOOM_WORK_DIR").is_some() || store.root().is_dir()
}

/// What [`try_claim`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// The lease is now this caller's: spawn the reconciler.
    Won,
    /// A live holder runs; `pending` is set so it makes one more pass.
    Queued,
    /// Throttled by the backoff, or the lock could not be written.
    Refused,
}

/// Decide and act in one critical section, so no holder can finish between a
/// skip and its `pending` mark: [`Claim::Won`] writes the caller's pid,
/// [`Claim::Queued`] sets `pending` on a live holder's line (its epoch and pid
/// untouched), [`Claim::Refused`] writes nothing.
fn try_claim(
    lock_path: &Path,
    now: u64,
    debounce_secs: u64,
    stale_lock_secs: u64,
    is_alive: impl Fn(u32) -> bool,
    pid: u32,
) -> Claim {
    update_lock(lock_path, |state| {
        match decide(lock_path, now, debounce_secs, stale_lock_secs, is_alive) {
            LockDecision::Spawn => (Some(held_by(state, now, pid)), Claim::Won),
            LockDecision::Skip => match state {
                Some(holder) if holder.pid != 0 => (mark_pending(holder), Claim::Queued),
                _ => (None, Claim::Refused),
            },
        }
    })
    .unwrap_or(Claim::Refused)
}

/// The fallible half of [`spawn_if_needed`]: check the target is trusted,
/// claim the lease (or queue behind its holder), spawn.
fn try_spawn(project_root: &Path, now: u64) -> Result<()> {
    let work_dir = WorkDir::new(project_root)?;
    let store = ContextStore::open(&work_dir)?;

    if !allowed_to_spawn(&store) {
        tracing::debug!(
            project_root = %project_root.display(),
            "reconcile-graph: refusing to spawn against an inferred project root with no existing cache"
        );
        return Ok(());
    }

    let lock_path = reconcile_lock_path(&store);

    let main_root = work_dir
        .main_project_root()
        .unwrap_or_else(|| project_root.to_path_buf());
    let config = RetrievalConfig::load(&main_root);

    let claim = try_claim(
        &lock_path,
        now,
        config.reconcile_debounce_secs,
        config.reconcile_stale_lock_secs,
        crate::process::is_process_alive,
        std::process::id(),
    );
    if claim != Claim::Won {
        // Throttled, queued behind a live holder, or the lock is unwritable —
        // never a second, uncoordinated reconcile on top of whatever holds it.
        return Ok(());
    }

    spawn_claimed(&lock_path, now, std::process::id(), |claim| {
        spawn_detached(project_root, claim)
    })
}

/// Run `spawn` for a lease `pid` just won at `now`. A failed spawn leaves no
/// child to release it, so the lease is freed here with a failure counted, and
/// the backoff throttles the next attempt instead of every prompt retrying. A
/// spawned child's pid replaces the spawning hook's (which exits right away)
/// so a second hook sees a live holder until the child re-stamps the lease.
fn spawn_claimed(
    lock_path: &Path,
    now: u64,
    pid: u32,
    spawn: impl FnOnce(u128) -> Result<Option<u32>>,
) -> Result<()> {
    let claim = read_lock(lock_path)
        .filter(|held| held.pid == pid && held.epoch == now)
        .map(|held| held.claim)
        .ok_or_else(|| anyhow::anyhow!("reconcile claim changed before spawn"))?;
    let child = spawn(claim).inspect_err(|_| release_failed(lock_path, now, pid, claim))?;
    if let Some(child_pid) = child {
        rebind_claim(lock_path, now, pid, child_pid, claim);
    }
    Ok(())
}

/// Suppress detached children in unit tests. Integration tests link a normal
/// library build and must call [`disable_spawn_for_tests`] before invoking
/// [`spawn_if_needed`]. The lease path remains exercised with spawning off.
static SPAWN_ENABLED: AtomicBool = AtomicBool::new(!cfg!(test));

/// Counts suppressed spawns without creating detached children.
#[cfg(test)]
static SUPPRESSED_SPAWNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Disable `spawn_detached` for the remainder of this process. Idempotent.
///
/// Public because integration tests use the normal library build.
pub fn disable_spawn_for_tests() {
    SPAWN_ENABLED.store(false, Ordering::SeqCst);
}

/// Launch `loom hook reconcile-graph` detached from this process: no stdio,
/// its own process group so it survives the hook's exit, and NEVER
/// awaited — `wait()`/`output()` here would turn a background self-heal into
/// a foreground stall on the hook's own latency budget.
///
/// Returns the child's pid; `None` when the test guard suppressed the spawn.
fn spawn_detached(project_root: &Path, claim: u128) -> Result<Option<u32>> {
    if !SPAWN_ENABLED.load(Ordering::SeqCst) {
        // See `SPAWN_ENABLED`'s doc: a test build must never create a real,
        // process-group-leading child that survives the test harness.
        #[cfg(test)]
        SUPPRESSED_SPAWNS.fetch_add(1, Ordering::SeqCst);
        return Ok(None);
    }

    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("loom"));
    let mut command = Command::new(program);
    command
        .args(["hook", "reconcile-graph"])
        .env("LOOM_RECONCILE_CLAIM", claim.to_string())
        .current_dir(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Passed through, not inherited implicitly: `Command` already inherits
    // the parent's environment by default, but naming these two explicitly
    // documents that they are load-bearing for A.22 (a stage's own scope
    // resolution) rather than incidental.
    for key in ["LOOM_STAGE_ID", "LOOM_WORK_DIR"] {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    Ok(Some(command.spawn()?.id()))
}

#[cfg(test)]
#[path = "tests_reconcile_graph.rs"]
mod tests;

#[cfg(test)]
mod tests_wants_rebuild;
