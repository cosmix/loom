//! Boundary teardown and settlement of the invocation record.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use crate::native_worker::boundary::Quiescence;
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

/// Settles a failure before bubblewrap was started (preflight, socket
/// directory, proxy, or a spawn refused before any process ran). This is the
/// only path that records `Released` without `verify_quiescent`: no namespace
/// exists, so there is nothing to prove empty.
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
