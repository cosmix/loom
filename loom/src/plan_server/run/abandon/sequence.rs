//! The abandon sequence: re-check the unmerged branches, write the marker, `loom stop`,
//! re-check, `loom clean --all`, confirm on disk, rename.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::commands::clean::unmerged_loom_branches;
use crate::plan_server::run::child::run_streamed;
use crate::planning::runtime::RuntimePaths;

use super::marker;
use super::plan::{leftover, run_stage_ids, running_plan, RunningPlan, Worktrees};
use super::retire::{rename_refusal, retire_step};
use super::AbandonOutcome;

const CLEAN_YES: &str = "LOOM_CLEAN_YES";
const STOP: &str = "loom stop";
const CLEAN: &str = "loom clean";
/// The line when `loom clean` exited 0 without removing the run.
pub(super) const CLEAN_LEFT_STATE: &str =
    "loom clean left the run's state in place; the plan keeps its name.";

/// Everything one abandon needs.
struct Abandon<'a> {
    root: &'a Path,
    runtime: &'a RuntimePaths,
    loom_bin: &'a Path,
    confirmed: &'a [(String, usize)],
    plan: RunningPlan,
}

/// What the sequence does with the marker once it ends.
enum Marker {
    /// The run's state is in place: nothing changed it.
    Remove,
    /// `loom clean` ran: the rename step removes the marker after a rename, and keeps it
    /// otherwise so "Finish abandoning" may follow.
    Keep,
}

pub(super) fn run(
    root: &Path,
    runtime: &RuntimePaths,
    loom_bin: &Path,
    confirmed: &[(String, usize)],
    on_line: &mut dyn FnMut(&str),
) -> Result<AbandonOutcome> {
    let Some(plan) = running_plan(root) else {
        let reason = "the workspace holds no IN_PROGRESS- plan to abandon".to_string();
        return Ok(AbandonOutcome::Refused { reason });
    };
    if let Some(stop) = branch_check(root, confirmed) {
        return Ok(stop);
    }
    // Refuse a rename that could never happen before anything destructive runs; the rename
    // re-checks every condition itself.
    if let Some(reason) = rename_refusal(root, &plan)? {
        return Ok(AbandonOutcome::Refused { reason });
    }
    marker::write(runtime, &plan)?;
    let abandon = Abandon {
        root,
        runtime,
        loom_bin,
        confirmed,
        plan,
    };
    let (outcome, fate) = abandon.steps(on_line);
    if matches!(fate, Marker::Remove) {
        marker::remove_logged(runtime, &abandon.plan);
    }
    outcome
}

/// Why the sequence must stop on the `loom/*` branches, or `None` to go on: `Refused` when they
/// cannot be listed (a git error counts as unmerged work, section 11), `BranchesChanged` with
/// the fresh sorted list when it differs from `confirmed`.
fn branch_check(root: &Path, confirmed: &[(String, usize)]) -> Option<AbandonOutcome> {
    let listing = unmerged_loom_branches(root);
    if let Some(error) = listing.listing_error {
        let reason = format!(
            "the loom/* branches cannot be listed, so any of them may hold unmerged work; \
             nothing was deleted: {error}"
        );
        return Some(AbandonOutcome::Refused { reason });
    }
    let mut now = listing.branches;
    let mut shown = confirmed.to_vec();
    now.sort();
    shown.sort();
    (now != shown).then_some(AbandonOutcome::BranchesChanged { now })
}

impl Abandon<'_> {
    fn steps(&self, on_line: &mut dyn FnMut(&str)) -> (Result<AbandonOutcome>, Marker) {
        let stage_ids = run_stage_ids(self.root);
        if let Some(failed) = self.child(STOP, &["stop"], false, on_line) {
            return (Ok(failed), Marker::Remove);
        }
        // A stage may have committed while the daemon stopped.
        if let Some(stop) = branch_check(self.root, self.confirmed) {
            match &stop {
                AbandonOutcome::Refused { reason } => on_line(reason),
                _ => on_line(
                    "The loom/* branches with unmerged commits changed while the daemon \
                     stopped; nothing was deleted. Confirm the new list to continue.",
                ),
            }
            return (Ok(stop), Marker::Remove);
        }
        // Every branch `loom clean` deletes was listed and confirmed (or there is none).
        if let Some(failed) = self.child(CLEAN, &["clean", "--all"], true, on_line) {
            return (Ok(failed), Marker::Remove);
        }
        if let Some(left) = leftover(self.root, Worktrees::OfRun(&stage_ids)) {
            on_line(CLEAN_LEFT_STATE);
            let reason = format!("{CLEAN_LEFT_STATE} ({left})");
            return (Ok(AbandonOutcome::Refused { reason }), Marker::Keep);
        }
        let outcome = retire_step(self.root, self.runtime, &self.plan, on_line);
        (outcome, Marker::Keep)
    }

    /// Run `loom <args>` in the project root, streaming its output; `Some(Failed)` unless it
    /// exits 0. `LOOM_CLEAN_YES` is set only where `clean_yes` says.
    fn child(
        &self,
        step: &str,
        args: &[&str],
        clean_yes: bool,
        on_line: &mut dyn FnMut(&str),
    ) -> Option<AbandonOutcome> {
        let mut cmd = Command::new(self.loom_bin);
        cmd.args(args).current_dir(self.root);
        if clean_yes {
            cmd.env(CLEAN_YES, "1");
        } else {
            cmd.env_remove(CLEAN_YES);
        }
        let code = match run_streamed(cmd, |_stream, line| on_line(line)) {
            Ok(status) => status.code().unwrap_or(-1),
            Err(error) => {
                on_line(&format!("{step} could not run to completion: {error}"));
                -1
            }
        };
        (code != 0).then(|| AbandonOutcome::Failed {
            step: step.to_string(),
            code,
        })
    }
}
