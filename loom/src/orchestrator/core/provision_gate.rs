//! The pre-spawn gate: the stage's `before_stage` checks, then the plan's
//! `provision` entries, both run on the host in the stage worktree.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, Result};

use crate::git::status_entries::{parse_porcelain_v1_z, StatusEntry, PORCELAIN_V1_Z};
use crate::git::worktree::WorktreeGit;
use crate::models::failure::FailureType;
use crate::models::stage::Stage;
use crate::orchestrator::busy::{self, BusyOp};
use crate::orchestrator::provision::{display_safe, read_provision_snapshot, run_provision};
use crate::orchestrator::tick::Phase;
use crate::plan::schema::ProvisionEntry;

use super::stage_block::StageBlock;
use super::Orchestrator;

/// Most paths a "left files git does not ignore" reason names.
const MAX_REPORTED_PATHS: usize = 10;

/// The block reason when the after-provision listing does not parse but the
/// before-provision one did: the provision named a file git cannot list as UTF-8.
const NON_UTF8_REASON: &str = "provision left a file whose name is not UTF-8 in the worktree; \
                               a provision command may write only files git ignores";

impl Orchestrator {
    /// Run the stage's `before_stage` gate on the pristine worktree, then provision it.
    /// `Ok(false)` means the stage was blocked and must not spawn.
    ///
    /// The entries come from the snapshot `loom init` took, never the plan file, which
    /// a sandboxed stage can edit. A `loom stop` during provisioning does not kill the
    /// command: it runs in its own process group and finishes or hits its timeout. The
    /// stage is still Queued (it is marked Executing only after the gates), so the next
    /// `loom run` provisions again; the commands are idempotent.
    pub(super) fn pre_spawn_gates_passed(
        &mut self,
        stage: &Stage,
        worktree_path: &Path,
        base_branch: &str,
    ) -> Result<bool> {
        // Before-stage checks first: they prove a delta only while the worktree is
        // pristine, and a provision that writes an unignored file would end that.
        if !self.before_stage_gate_passed(stage, worktree_path, base_branch)? {
            return Ok(false);
        }
        let entries = match read_provision_snapshot(&self.config.work_dir) {
            Ok(entries) => entries,
            Err(e) => {
                let reason = format!("provision: cannot read the snapshot: {e:#}");
                return self.block_for_provision(&stage.id, reason);
            }
        };
        if entries.is_empty() {
            return Ok(true);
        }
        println!(
            "  Provisioning '{}' ({} command(s))...",
            stage.id,
            entries.len()
        );
        match provision_worktree(&self.config.work_dir, &stage.id, &entries, worktree_path) {
            Ok(()) => Ok(true),
            Err(reason) => self.block_for_provision(&stage.id, reason),
        }
    }

    /// Block the stage with `reason`. Recording `failure_info` keeps `loom status` from
    /// reading the block as an agent's; an infrastructure error is never auto-retried.
    /// `Ok(false)` once the block persisted; `Err` naming the stage when it did not, so
    /// the scheduling report records the stage as ownership-uncertain instead of
    /// leaving it Queued with no visible reason.
    fn block_for_provision(&mut self, stage_id: &str, reason: String) -> Result<bool> {
        eprintln!("Provisioning failed for '{stage_id}': {reason}");
        let evidence = reason.lines().map(str::to_string).collect();
        let block = StageBlock {
            close_reason: Some(reason),
            ..StageBlock::new(FailureType::InfrastructureError, evidence)
        };
        if !self.block_stage(stage_id, block) {
            return Err(anyhow!(
                "provision failed for stage '{stage_id}' and the Blocked state could not be persisted"
            ));
        }
        Ok(false)
    }
}

/// Run `entries` in `worktree_path`, then check they left no file git does not ignore.
/// `Err` is the stage's block reason.
///
/// Each entry runs in its own [`busy::while_busy`] wrap, so the daemon tick stays fresh
/// and names the install for `stage_id`. `PROVISION_TIMEOUT` bounds one command, so one
/// wrap per entry keeps every wrap inside the provisioning ceiling, where a single wrap
/// around all entries would let several healthy slow ones read as a stall.
fn provision_worktree(
    work_dir: &Path,
    stage_id: &str,
    entries: &[ProvisionEntry],
    worktree_path: &Path,
) -> Result<(), String> {
    let before = status_entries(work_dir, worktree_path);
    entries.iter().try_for_each(|entry| {
        busy::while_busy(
            work_dir,
            Phase::Spawning,
            BusyOp::Provisioning,
            stage_id,
            || run_provision(std::slice::from_ref(entry), worktree_path),
        )
    })?;
    // A listing git could not produce (a plain directory, a worktree that cannot be
    // pinned) skips the comparison, as `find_prior_stage_work` does; so does a
    // before-listing that does not parse, since the provision did not cause it.
    let Some(Ok(before)) = before else {
        return Ok(());
    };
    let after = match status_entries(work_dir, worktree_path) {
        None => return Ok(()),
        Some(Err(error)) => {
            tracing::warn!(worktree = %worktree_path.display(), "{error:#}");
            return Err(NON_UTF8_REASON.to_string());
        }
        Some(Ok(after)) => after,
    };
    let known: HashSet<_> = before.iter().map(entry_key).collect();
    let added: Vec<&StatusEntry> = after
        .iter()
        .filter(|entry| !known.contains(&entry_key(entry)))
        .collect();
    if added.is_empty() {
        Ok(())
    } else {
        Err(unignored_reason(&added))
    }
}

/// A status entry's identity for the before/after comparison.
fn entry_key(entry: &StatusEntry) -> ([u8; 2], &str, Option<&str>) {
    (entry.code, &entry.path, entry.origin.as_deref())
}

/// The worktree's `git status` entries, untracked files listed one by one and ignored
/// files not at all. `None` when git cannot list them; `Some(Err)` when the listing is
/// not strict porcelain (a non-UTF-8 name). Pinned: this runs on the host, and the stage
/// controls the worktree's `.git` file.
fn status_entries(
    work_dir: &Path,
    worktree_path: &Path,
) -> Option<anyhow::Result<Vec<StatusEntry>>> {
    let listed = WorktreeGit::pinned_in_project_of(work_dir, worktree_path)
        .and_then(|git| git.run(&PORCELAIN_V1_Z));
    // Raw output: a trimmed listing would eat the leading space of a first ` M <path>`
    // entry and shift every column after it.
    match listed {
        Ok(output) if output.status.success() => Some(parse_porcelain_v1_z(&output.stdout)),
        Ok(output) => {
            warn_status_unreadable(worktree_path, &String::from_utf8_lossy(&output.stderr));
            None
        }
        Err(e) => {
            warn_status_unreadable(worktree_path, &format!("{e:#}"));
            None
        }
    }
}

fn warn_status_unreadable(worktree_path: &Path, error: &str) {
    tracing::warn!(
        worktree = %worktree_path.display(),
        error = %error.trim(),
        "Could not read worktree status around provisioning"
    );
}

/// The block reason naming the files a provision left that git does not ignore. Each
/// name is [`display_safe`]: `git status -z` prints it raw, and a stage chooses it.
fn unignored_reason(added: &[&StatusEntry]) -> String {
    let shown: Vec<String> = added
        .iter()
        .map(|entry| display_safe(&entry.path))
        .take(MAX_REPORTED_PATHS)
        .collect();
    let mut listed = shown.join(", ");
    let more = added.len().saturating_sub(MAX_REPORTED_PATHS);
    if more > 0 {
        listed.push_str(&format!(", and {more} more"));
    }
    format!(
        "provision left files git does not ignore in the worktree: {listed}; a provision \
         command may write only files git ignores (add them to .gitignore or change the command)"
    )
}

#[cfg(test)]
#[path = "provision_gate_tests.rs"]
mod tests;
