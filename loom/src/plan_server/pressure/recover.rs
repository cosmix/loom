//! What a pressure run leaves behind: the decision on a recorded Codex process, and the report
//! of a run the last server never finished.
//!
//! Plan 7's start-up recovery tears a live pressure session down and terminates its verified
//! `codex` child; this module deletes that run's Codex report and names the run on the terminal.
//! Missing evidence never destroys: a process whose start time cannot be verified is left alone.

use std::path::{Component, Path};
use std::thread;
use std::time::{Duration, Instant};

use super::baseline;
use super::codex::{CONFIG_CHANGED, NOT_EXITED};
use super::config_guard::{changed, snapshot};
use crate::plan_server::lifecycle::safe;
use crate::plan_session::bridge::RecoveryReport;
use crate::plan_session::kind::SessionKind;
use crate::planning::guidance::StartBlocks;
use crate::planning::runtime::RuntimePaths;
use crate::pressure::paths::codex_report_path;
use crate::process::{
    terminate_verified, verify_process_identity, IdentityStatus, ProcessIdentity,
};

/// What may be done with a recorded Codex process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexAction {
    /// The process is alive and its start time matches the record.
    Terminate,
    /// The process is alive but its start time cannot be compared: it may not be ours.
    LeaveAlone,
    /// Nothing was recorded, or the recorded process is gone.
    Nothing,
}

/// Decide from the recorded identity; only a verified identity may be signalled.
pub fn codex_action(identity: Option<ProcessIdentity>) -> CodexAction {
    match identity.map(verify_process_identity) {
        Some(IdentityStatus::VerifiedAlive) => CodexAction::Terminate,
        Some(IdentityStatus::Unverifiable) => CodexAction::LeaveAlone,
        Some(IdentityStatus::Dead) | None => CodexAction::Nothing,
    }
}

/// Terminate a recorded Codex process when its identity verifies; a line for the terminal
/// when it was left alone or the signal failed.
pub(super) fn terminate_codex(identity: ProcessIdentity) -> Option<String> {
    match codex_action(Some(identity)) {
        CodexAction::Terminate => terminate_verified(identity).err().map(|error| {
            format!(
                "pressure run: Codex (pid {}) was not terminated: {}",
                identity.pid,
                safe(&format!("{error:#}"))
            )
        }),
        CodexAction::LeaveAlone => Some(format!(
            "pressure run: left Codex (pid {}) running: its start time cannot be verified",
            identity.pid
        )),
        CodexAction::Nothing => None,
    }
}

/// For every pressure session recovery ended: delete `codex-<plan file>` beside its plan and
/// return the line naming the interrupted run, one per plan.
pub(super) fn interrupted_runs(root: &Path, report: &RecoveryReport) -> Vec<String> {
    let mut plans: Vec<&Path> = Vec::new();
    for session in report.torn_down.iter().chain(&report.closed) {
        if let SessionKind::Pressure { plan } = &session.kind {
            if !plans.contains(&plan.as_path()) {
                plans.push(plan.as_path());
            }
        }
    }
    plans
        .into_iter()
        .map(|plan| {
            delete_report(root, plan);
            format!(
                "pressure run on {} was interrupted when the last server stopped",
                safe(&plan.display().to_string())
            )
        })
        .collect()
}

/// The plan path comes from a session record on disk: only a plain path under `doc/plans/`
/// may steer a deletion.
fn delete_report(root: &Path, plan: &Path) {
    let plain = plan
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if !plain || !plan.starts_with("doc/plans") {
        return;
    }
    let report = codex_report_path(&root.join(plan));
    if let Err(error) = crate::claude::remove_if_exists(&report) {
        tracing::warn!(%error, "cannot delete an interrupted run's Codex report");
    }
}

/// How often a Codex recovery just signalled is looked at.
const EXIT_POLL: Duration = Duration::from_millis(100);

/// The configuration check the last server never made: when a baseline file exists, wait for
/// the recovered Codex children to exit (recovery has only signalled them), compare, and block
/// later launches on a change. The terminal line, or `None` when no baseline exists or nothing
/// changed. A run that started during the wait does not cancel the check: the block is still
/// written, and only the new run's baseline file stays. A block that could not be saved keeps
/// the baseline for the next start.
pub(super) fn check_baseline(
    root: &Path,
    runtime: &RuntimePaths,
    report: &RecoveryReport,
    grace: Duration,
) -> Option<String> {
    let (seen, before) = match baseline::load(runtime) {
        Ok(None) => return None,
        Ok(Some(seen)) => {
            let parsed = seen.snapshot();
            (Some(seen), parsed)
        }
        Err(error) => (None, Err(error)),
    };
    if codex_still_running(report, grace) {
        return block_and_report(runtime, seen.as_ref(), NOT_EXITED.to_owned(), &[]);
    }
    let compared = before.and_then(|before| {
        let after = snapshot(root)?;
        Ok(changed(&before, &after))
    });
    let (reason, paths) = match compared {
        Ok(paths) if paths.is_empty() => {
            if let Err(error) = baseline::settle(runtime, seen.as_ref(), || Ok(())) {
                tracing::warn!(%error, "cannot remove the configuration baseline");
            }
            return None;
        }
        Ok(paths) => (format!("{CONFIG_CHANGED}: {}", paths.join(", ")), paths),
        Err(error) => {
            let why = format!("cannot be checked after the last server stopped ({error:#})");
            (format!("{CONFIG_CHANGED}: {why}"), Vec::new())
        }
    };
    block_and_report(runtime, seen.as_ref(), reason, &paths)
}

/// Wait up to `grace` for every Codex child of a recovered pressure session to leave;
/// `true` when one is still alive or cannot be verified.
fn codex_still_running(report: &RecoveryReport, grace: Duration) -> bool {
    let children: Vec<ProcessIdentity> = report
        .torn_down
        .iter()
        .chain(&report.closed)
        .filter(|session| matches!(session.kind, SessionKind::Pressure { .. }))
        .flat_map(|session| &session.children)
        .filter(|child| child.label == "codex")
        .map(|child| child.identity())
        .collect();
    let deadline = Instant::now() + grace;
    loop {
        let alive = children
            .iter()
            .any(|identity| !matches!(verify_process_identity(*identity), IdentityStatus::Dead));
        if !alive {
            return false;
        }
        if Instant::now() >= deadline {
            return true;
        }
        thread::sleep(EXIT_POLL);
    }
}

/// Persist the block; the baseline goes only once the block is saved, so a failed save is
/// checked again at the next start. A baseline another run replaced meanwhile is left in place,
/// but the block is still written.
fn block_and_report(
    runtime: &RuntimePaths,
    seen: Option<&baseline::Loaded>,
    reason: String,
    changed: &[String],
) -> Option<String> {
    match baseline::settle(runtime, seen, || {
        StartBlocks::block(runtime, &reason, changed)
    }) {
        Ok(_) => Some(format!("pressure run: {}", safe(&reason))),
        Err(error) => Some(format!(
            "pressure run: the configuration block was not saved: {}",
            safe(&format!("{error:#}"))
        )),
    }
}
