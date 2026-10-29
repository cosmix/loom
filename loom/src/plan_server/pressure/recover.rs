//! What a pressure run leaves behind: the decision on a recorded Codex process, and the report
//! of a run the last server never finished.
//!
//! Plan 7's start-up recovery tears a live pressure session down and terminates its verified
//! `codex` child; this module deletes that run's Codex report and names the run on the terminal.
//! Missing evidence never destroys: a process whose start time cannot be verified is left alone.

use std::path::{Component, Path};

use crate::plan_server::lifecycle::safe;
use crate::plan_session::bridge::RecoveryReport;
use crate::plan_session::kind::SessionKind;
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
