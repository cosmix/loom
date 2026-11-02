//! The pre-spawn gate: the stage's `before_stage` checks, then the plan's
//! `provision` entries, both run on the host in the stage worktree.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;

use crate::git::worktree::WorktreeGit;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::provision::{display_safe, read_provision_snapshot, run_provision};
use crate::orchestrator::tick::{self, Phase};
use crate::plan::schema::ProvisionEntry;

use super::persistence::Persistence;
use super::Orchestrator;

/// How often the provision thread re-stamps the daemon tick. It stays well under
/// `tick::STALL_THRESHOLD_SECS`, so `loom status` never reads an install as a stall.
const TICK_INTERVAL: Duration = Duration::from_secs(10);

/// Most paths a "left files git does not ignore" reason names.
const MAX_REPORTED_PATHS: usize = 10;

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
        match provision_worktree(&self.config.work_dir, &entries, worktree_path) {
            Ok(()) => Ok(true),
            Err(reason) => self.block_for_provision(&stage.id, reason),
        }
    }

    /// Block the stage with `reason`. Recording `failure_info` keeps `loom status` from
    /// reading the block as an agent's; an infrastructure error is never auto-retried.
    fn block_for_provision(&mut self, stage_id: &str, reason: String) -> Result<bool> {
        eprintln!("Provisioning failed for '{stage_id}': {reason}");
        self.update_stage(stage_id, |current| {
            current.try_mark_blocked()?;
            current.failure_info = Some(FailureInfo {
                failure_type: FailureType::InfrastructureError,
                detected_at: Utc::now(),
                evidence: reason.lines().map(str::to_string).collect(),
            });
            current.close_reason = Some(reason.clone());
            Ok(())
        })?;
        let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
        Ok(false)
    }
}

/// Run `entries` in `worktree_path`, then check they left no file git does not ignore.
/// `Err` is the stage's block reason.
fn provision_worktree(
    work_dir: &Path,
    entries: &[ProvisionEntry],
    worktree_path: &Path,
) -> Result<(), String> {
    let before = status_entries(work_dir, worktree_path);
    run_with_fresh_tick(work_dir, entries, worktree_path)?;
    // A listing that failed (a plain directory, a worktree that cannot be pinned) skips the
    // comparison, as `find_prior_stage_work` does.
    let (Some(before), Some(after)) = (before, status_entries(work_dir, worktree_path)) else {
        return Ok(());
    };
    let known: HashSet<&str> = before.iter().map(String::as_str).collect();
    let added: Vec<&str> = after
        .iter()
        .map(String::as_str)
        .filter(|entry| !known.contains(entry))
        .collect();
    if added.is_empty() {
        Ok(())
    } else {
        Err(unignored_reason(&added))
    }
}

/// [`run_provision`] with a second thread that re-stamps the daemon tick: the tick is
/// stamped before the stage loop, and a stale one makes `loom status` tell the operator
/// to restart the daemon mid-install.
fn run_with_fresh_tick(
    work_dir: &Path,
    entries: &[ProvisionEntry],
    worktree_path: &Path,
) -> Result<(), String> {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let ticker = scope.spawn(|| keep_tick_fresh(work_dir, &done));
        // The guard stops the ticker even when `run_provision` panics; otherwise the
        // scope would wait on it forever and hang the daemon.
        let _stop = StopTicker {
            done: &done,
            ticker: ticker.thread(),
        };
        run_provision(entries, worktree_path)
    })
}

/// Sets the ticker's stop flag and wakes it when dropped.
struct StopTicker<'a> {
    done: &'a AtomicBool,
    ticker: &'a std::thread::Thread,
}

impl Drop for StopTicker<'_> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.ticker.unpark();
    }
}

/// Stamp the tick now, then every [`TICK_INTERVAL`] until `done` is set.
fn keep_tick_fresh(work_dir: &Path, done: &AtomicBool) {
    tick::record(work_dir, Phase::Spawning);
    let mut stamped = Instant::now();
    while !done.load(Ordering::Acquire) {
        std::thread::park_timeout(Duration::from_secs(1));
        if stamped.elapsed() >= TICK_INTERVAL {
            tick::record(work_dir, Phase::Spawning);
            stamped = Instant::now();
        }
    }
}

/// The worktree's `git status` entries, untracked files listed one by one and ignored
/// files not at all. `None` when git cannot list them. Pinned: this runs on the host,
/// and the stage controls the worktree's `.git` file.
fn status_entries(work_dir: &Path, worktree_path: &Path) -> Option<Vec<String>> {
    let args = ["status", "--porcelain=v1", "-z", "--untracked-files=all"];
    let listed =
        WorktreeGit::pinned_in_project_of(work_dir, worktree_path).and_then(|git| git.run(&args));
    // Raw output: a trimmed listing would eat the leading space of a first ` M <path>`
    // entry and shift every column after it.
    let output = match listed {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            warn_status_unreadable(worktree_path, &String::from_utf8_lossy(&output.stderr));
            return None;
        }
        Err(e) => {
            warn_status_unreadable(worktree_path, &format!("{e:#}"));
            return None;
        }
    };
    Some(split_status_entries(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn warn_status_unreadable(worktree_path: &Path, error: &str) {
    tracing::warn!(
        worktree = %worktree_path.display(),
        error = %error.trim(),
        "Could not read worktree status around provisioning"
    );
}

/// Split `git status --porcelain=v1 -z` output into one string per entry. A rename or
/// copy (`R`/`C` in either status column) is followed by a second NUL-separated field,
/// the origin path with no `XY ` prefix; it belongs to the same entry.
fn split_status_entries(listing: &str) -> Vec<String> {
    let mut fields = listing.split('\0').filter(|field| !field.is_empty());
    let mut entries = Vec::new();
    while let Some(field) = fields.next() {
        let origin = if field.get(..2).is_some_and(|xy| xy.contains(['R', 'C'])) {
            fields.next()
        } else {
            None
        };
        entries.push(match origin {
            Some(origin) => format!("{field}\0{origin}"),
            None => field.to_string(),
        });
    }
    entries
}

/// The block reason naming the files a provision left that git does not ignore. Each
/// name is [`display_safe`]: `git status -z` prints it raw, and a stage chooses it.
fn unignored_reason(added: &[&str]) -> String {
    // A status entry is "XY <path>", plus "\0<origin>" for a rename or copy.
    let shown: Vec<String> = added
        .iter()
        .map(|&entry| entry.get(3..).unwrap_or(entry))
        .map(|path| display_safe(path.split('\0').next().unwrap_or(path)))
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
