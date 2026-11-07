//! The sweep that ends every verdict pass: a stage the verdict left held
//! (`NeedsAdjudication`, no session, every dispute applied) gets the daemon's
//! completion run on its committed tree, or goes back to a fresh session when
//! that run cannot decide it.
//!
//! It reads the stage files, never `self.graph`: the graph's statuses were
//! synced earlier in the tick, before this pass applied the verdicts.

use std::path::Path;

use chrono::Utc;

use crate::host_verify::verdict_complete::{submit_verdict_completion, VerdictStart};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::adjudication::{
    hand_back_held_stage, is_held_for_completion, is_verdict_held,
};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::transitions::list_all_stages;

use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// Start the completion run of every verdict-held stage, and hand back
    /// each one the operator holds (`loom stage hold`): the daemon never
    /// completes those. Handing back takes the stage's dispute lock, so this
    /// runs only once every claim of the verdict pass has dropped.
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
            if stage.status != StageStatus::NeedsAdjudication || stage.session.is_some() {
                continue;
            }
            if stage.held {
                // Only a verdict-held stage is handed back: taking the dispute
                // lock for a stage with an open dispute would change nothing.
                if !is_verdict_held(&work_dir, &stage) {
                    continue;
                }
                let message = "it is on hold, so loom does not run its completion; a fresh \
                               session starts with the verdict once it is released";
                hand_back(&work_dir, &stage.id, &[], message);
            } else if is_held_for_completion(&work_dir, &stage) {
                start_verdict_completion(&work_dir, &stage);
            }
        }
    }
}

/// Submit the held stage's completion run, or hand the stage back when it
/// cannot have one. An error hands it back too, with the error as its one
/// failure, so a lasting error never strands the stage.
fn start_verdict_completion(work_dir: &Path, stage: &Stage) {
    let (reason, failures) = match submit_verdict_completion(work_dir, stage, Utc::now()) {
        Ok(VerdictStart::Waiting(job)) => {
            tracing::debug!(target: "loom::adjudication", stage = %stage.id, job = %job,
                "the verdict completion job still decides the stage");
            return;
        }
        Ok(VerdictStart::Submitted(job)) => {
            clear_status_line();
            eprintln!(
                "Stage '{}': the verdict changed nothing in the plan; loom runs its completion \
                 on the committed tree (host job {job})",
                stage.id
            );
            return;
        }
        Ok(VerdictStart::Ineligible(reason)) => (reason, Vec::new()),
        Err(error) => {
            let line = run_output_line(&format!("{error:#}"));
            (line.clone(), vec![line])
        }
    };
    let message = format!("{reason}; a fresh session starts with the verdict");
    hand_back(work_dir, &stage.id, &failures, &message);
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
