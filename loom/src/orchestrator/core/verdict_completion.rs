//! The sweep that ends every verdict pass: a stage the verdict left held
//! (`NeedsAdjudication`, no session, every dispute applied) gets the daemon's
//! completion run on its committed tree, or goes back to a fresh session when
//! that run cannot decide it.
//!
//! It reads the stage files, never `self.graph`: the graph's statuses were
//! synced earlier in the tick, before this pass applied the verdicts.
//!
//! Lock order, per stage: the admission lock, then the dispute lock and the
//! stage file (a hand-back) or the host-job store (a submission), the order
//! `loom stage reset` takes them in. `flock` descriptors in one process
//! conflict, so the sweep runs only where none of these is held: after the
//! verdict pass has dropped every dispute claim, from a caller holding no
//! lock (`initialize_run`, `run_tick`).

use std::os::fd::OwnedFd;
use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::fs::stage_admission;
use crate::host_verify::verdict_complete::{submit_verdict_completion, VerdictStart};
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::{hand_back_held_stage, is_verdict_held};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::transitions::{list_all_stages, load_stage};

use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// Start the completion run of every verdict-held stage, and hand back
    /// each one the operator holds (`loom stage hold`) once no job decides
    /// it: the daemon never completes those.
    pub(super) fn start_verdict_completions(&mut self) {
        let work_dir = self.config.work_dir.clone();
        let stages = match list_all_stages(&work_dir) {
            Ok(stages) => stages,
            Err(error) => {
                tracing::warn!(target: "loom::adjudication", error = %format!("{error:#}"),
                    "could not list stages for verdict completions; retrying next tick");
                return;
            }
        };
        for stage in stages {
            // Checked again under the lock; a stage with an open dispute, a
            // session or another status takes no lock at all.
            if is_verdict_held(&work_dir, &stage) {
                start_verdict_completion(&work_dir, &stage.id);
            }
        }
    }
}

/// Submit the held stage's completion run, or hand the stage back when it
/// cannot have one. An error hands it back too, with the error as its one
/// failure, so a lasting error never strands the stage.
fn start_verdict_completion(work_dir: &Path, stage_id: &str) {
    let (_admission, stage) = match admitted_stage(work_dir, stage_id) {
        Ok(admitted) => admitted,
        Err(error) => {
            tracing::warn!(target: "loom::adjudication", stage = %stage_id,
                error = %format!("{error:#}"),
                "could not admit the verdict-held stage to its completion; retrying next tick");
            return;
        }
    };
    let (reason, failures) = match submit_verdict_completion(work_dir, &stage, Utc::now()) {
        Ok(VerdictStart::NotHeld) => return,
        Ok(VerdictStart::Waiting(job)) => {
            tracing::debug!(target: "loom::adjudication", stage = %stage_id, job = %job,
                "the verdict completion job still decides the stage");
            return;
        }
        Ok(VerdictStart::Submitted(job)) => {
            clear_status_line();
            eprintln!(
                "Stage '{stage_id}': the verdict changed nothing in the plan; loom runs its \
                 completion on the committed tree (host job {job})"
            );
            return;
        }
        Ok(VerdictStart::Ineligible(reason)) => (reason, Vec::new()),
        Err(error) => {
            let line = run_output_line(&format!("{error:#}"));
            (line.clone(), vec![line])
        }
    };
    let release = if stage.held {
        " once it is released"
    } else {
        ""
    };
    let message = format!("{reason}; a fresh session starts with the verdict{release}");
    hand_back(work_dir, stage_id, &failures, &message);
}

/// The stage's admission lock and the stage reloaded under it. `loom stage
/// reset` holds that lock from reading the stage's job through cancelling it,
/// so a reset either ran first, and the reloaded stage is no longer held, or
/// runs after the submission and cancels the job queued here.
fn admitted_stage(work_dir: &Path, stage_id: &str) -> Result<(OwnedFd, Stage)> {
    let admission = stage_admission::lock_for_daemon(work_dir, stage_id)?;
    Ok((admission, load_stage(stage_id, work_dir)?))
}

/// Re-queue the held stage for a fresh session and tell the operator
/// `message`. A failure is logged; the next tick's sweep tries again.
fn hand_back(work_dir: &Path, stage_id: &str, failures: &[String], message: &str) {
    match hand_back_held_stage(work_dir, stage_id, failures) {
        Ok(true) => {
            clear_status_line();
            eprintln!("Stage '{stage_id}': {message}");
        }
        Ok(false) => {}
        Err(error) => tracing::warn!(target: "loom::adjudication", stage = %stage_id,
            error = %format!("{error:#}"),
            "could not hand the verdict-held stage to a fresh session; retrying next tick"),
    }
}

#[cfg(test)]
mod tests;
