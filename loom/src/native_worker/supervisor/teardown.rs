//! Boundary teardown and settlement of the invocation record.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use crate::native_worker::boundary::{Quiescence, SpawnedBoundary};
use crate::native_worker::model::{NativeEvent, Outcome, EOF_GRACE_SECS};
use crate::native_worker::store::EventLog;
use crate::process::OwnedProcess;

/// Stops the outer bubblewrap once the caller has closed its input: a grace of
/// `EOF_GRACE_SECS` for the app-server to exit on EOF, then always a SIGKILL of
/// the process group (`--die-with-parent` takes the pid namespace down with it).
/// There is no TERM stage: the provider has nothing to flush that loom reads.
pub(crate) fn stop_process(process: &mut OwnedProcess) {
    let _ = process.wait(Duration::from_secs(EOF_GRACE_SECS));
    process.retire();
}

/// Settles `boundary`, whose identity could not be recorded (`error`), once
/// [`stop_unrecorded`] stopped it. An `Unverified` ownership names the
/// identity error before the reason the namespace was not proven empty.
pub(super) fn settle_unrecorded(
    dir: &Path,
    boundary: SpawnedBoundary,
    error: &anyhow::Error,
) -> ExitCode {
    let quiescence = match stop_unrecorded(boundary) {
        Quiescence::Unverified(detail) => {
            Quiescence::Unverified(format!("no boundary identity ({error:#}): {detail}"))
        }
        released => released,
    };
    let reason = format!("could not record boundary: {error:#}");
    settle_exit(dir, Outcome::Unknown, &reason, quiescence)
}

/// Stops `boundary`, whose identity could not be recorded, then kills its
/// namespace by the pid and inode bubblewrap reported. The scratch goes only
/// when that proves the namespace empty; otherwise it stays for the sweep.
fn stop_unrecorded(mut boundary: SpawnedBoundary) -> Quiescence {
    // Closing the input first lets the app-server exit on EOF instead of
    // `stop_process` waiting out the whole grace.
    drop(boundary.stdin);
    stop_process(&mut boundary.process);
    #[cfg(target_os = "linux")]
    {
        use crate::native_worker::boundary::{keep_scratch_unless_released, kill_namespace};
        let quiescence = kill_namespace(boundary.namespace_pid, boundary.pid_namespace);
        keep_scratch_unless_released(&mut boundary.scratch, &quiescence);
        quiescence
    }
    #[cfg(not(target_os = "linux"))]
    Quiescence::Unverified("pid namespaces need Linux".into())
}

/// Appends `Terminal { outcome, reason }`, then the ownership event `quiescence`
/// names. The outcome is recorded as given: an `Unverified` ownership never
/// changes it, and a `Released` never upgrades it.
pub fn settle(
    dir: &Path,
    outcome: Outcome,
    reason: &str,
    quiescence: Quiescence,
) -> anyhow::Result<()> {
    let mut log = EventLog::open(dir)?;
    log.append(NativeEvent::Terminal {
        outcome,
        reason: reason.to_string(),
    })?;
    log.append(match quiescence {
        Quiescence::Released => NativeEvent::OwnershipReleased {
            by: "supervisor".into(),
        },
        Quiescence::Unverified(detail) => NativeEvent::OwnershipUnverified { detail },
    })?;
    Ok(())
}

/// Settles a failure that left no namespace process (preflight, socket
/// directory, proxy, a spawn refused before any process ran, or a bubblewrap
/// that exited before creating its namespace or had it proven empty). The
/// only path that records `Released` without a namespace scan of its own:
/// nothing is left to prove empty.
pub(super) fn settle_unspawned(dir: &Path, reason: &str) -> ExitCode {
    settle_exit(dir, Outcome::Failed, reason, Quiescence::Released)
}

/// Settles a failed `boundary::spawn`. `started` (from
/// `boundary::spawn_started`) means bubblewrap ran before the failure: its
/// namespace may hold a process outside the retired group, and with no
/// identity recorded nothing can prove it gone, so ownership is `Unverified`.
pub(super) fn settle_spawn_failure(dir: &Path, error: &str, started: bool) -> ExitCode {
    let reason = format!("boundary spawn failed: {error}");
    if !started {
        return settle_unspawned(dir, &reason);
    }
    let detail = format!(
        "bubblewrap was started but reported no pid namespace, so its processes cannot be \
         proven stopped: {error}"
    );
    settle_exit(
        dir,
        Outcome::Failed,
        &reason,
        Quiescence::Unverified(detail),
    )
}

/// `settle`, as the supervisor's exit code: 1 when the record could not be
/// appended (the daemon then proves the namespace empty and settles itself).
pub(super) fn settle_exit(
    dir: &Path,
    outcome: Outcome,
    reason: &str,
    quiescence: Quiescence,
) -> ExitCode {
    match settle(dir, outcome, reason, quiescence) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("settle failed: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "teardown_tests.rs"]
mod tests;
