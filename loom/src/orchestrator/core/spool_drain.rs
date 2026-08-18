//! Drains sandboxed-worktree memory spools into the canonical journal.
//!
//! `loom memory note` writes directly to `.work/memory/<stage>.md` via
//! [`crate::fs::memory::append_entry`]. Inside a sandboxed worktree that
//! write is refused (`.work` is a symlink out of the worktree; the sandbox
//! grants no write there), so the CLI falls back to appending the entry to
//! `<worktree>/.loom/memory-spool.jsonl` instead — see [`crate::fs::memory`]
//! for the full spool design. This module is the daemon side: on every poll
//! tick it drains every stage's pending spool into the real journal.
//!
//! Attribution is derived ONLY from which worktree a spool file was found
//! in, never from the spool payload itself (which deliberately carries no
//! stage id). Memory files are injected into OTHER stages' prompts by
//! `orchestrator/signals/generate.rs`, so mis-attributing an entry would be a
//! prompt-injection channel between stages.

use std::path::Path;

use crate::fs::memory::{self, DrainOutcome};
use crate::models::worktree::Worktree;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    /// Drain every stage's pending memory spool into its canonical journal.
    ///
    /// Stages are enumerated by a disk scan of `.work/stages/`, mirroring
    /// `spawn_merge_resolution_sessions` — disk is the source of truth and
    /// this survives daemon restarts, unlike `active_worktrees` (never
    /// repopulated for stages recovered as still-Executing) or
    /// `active_sessions` (misses recovered stages).
    ///
    /// Deliberately NOT filtered by stage status: `loom stage complete` is
    /// the last act of a stage session, so entries recorded moments before
    /// completion are still pending when the stage leaves `Executing`. A
    /// status filter would strand exactly the most valuable notes.
    pub(super) fn drain_stage_spools(&mut self) {
        let stages_dir = self.config.work_dir.join("stages");
        let entries = match std::fs::read_dir(&stages_dir) {
            Ok(entries) => entries,
            Err(_) => return, // no stages directory yet - nothing to drain
        };

        let stage_ids: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    return None;
                }
                let filename = path.file_name().and_then(|s| s.to_str())?;
                crate::fs::stage_files::extract_stage_id(filename)
            })
            .collect();

        for stage_id in stage_ids {
            // Enumerating the stage file already validates the id maps to a
            // real stage, which is the property we actually need; a failed
            // load just means a transiently unreadable/corrupt file, not a
            // reason to skip draining forever, so try again next tick.
            if self.load_stage(&stage_id).is_err() {
                continue;
            }
            let worktree_root = Worktree::worktree_path(&self.config.repo_root, &stage_id);
            self.drain_one_stage_spool(&stage_id, &worktree_root);
        }
    }

    /// Drain one stage's spool (if any) into its canonical journal.
    ///
    /// The validation and attribution logic itself lives in
    /// [`memory::drain_into_journal`], shared with the final pre-teardown
    /// drain in `git::cleanup::batch::cleanup_after_merge`; this method adds
    /// only the daemon-tick logging and error bookkeeping on top.
    ///
    /// Cheap no-op when the stage has no spool file: `memory::drain_into_journal`
    /// returns `DrainOutcome::default()` without touching disk, so calling
    /// this unconditionally for every stage on every tick is fine.
    pub(super) fn drain_one_stage_spool(
        &mut self,
        stage_id: &str,
        worktree_root: &Path,
    ) -> DrainOutcome {
        let work_dir = self.config.work_dir.clone();
        let result = memory::drain_into_journal(&work_dir, stage_id, worktree_root);

        self.record_drain_result(stage_id, worktree_root, result)
    }

    /// Log and account for the outcome of one stage's drain pass.
    fn record_drain_result(
        &mut self,
        stage_id: &str,
        worktree_root: &Path,
        result: anyhow::Result<DrainOutcome>,
    ) -> DrainOutcome {
        match result {
            Ok(outcome) => {
                // Silence when nothing was pending - this runs every 5
                // seconds for every stage, and the common case is an
                // absent spool file.
                if outcome.drained > 0 || outcome.skipped_malformed > 0 {
                    tracing::info!(
                        stage_id = %stage_id,
                        appended = outcome.drained,
                        skipped_malformed = outcome.skipped_malformed,
                        "Drained memory spool"
                    );
                }
                // A later recurrence of a drain failure for this stage should
                // be reported again, mirroring `verified_merged`'s
                // invalidate-on-change pattern.
                self.spool_drain_error_logged.remove(stage_id);
                outcome
            }
            Err(e) => {
                // Log-once per stage so a stuck spool (e.g. append_entry
                // hitting a permissions problem in .work/memory) doesn't
                // flood the logs on every 5-second poll.
                if self.spool_drain_error_logged.insert(stage_id.to_string()) {
                    tracing::warn!(
                        stage_id = %stage_id,
                        worktree_root = %worktree_root.display(),
                        error = %e,
                        "Failed to drain memory spool; entries remain pending and will \
                         retry next tick"
                    );
                }
                DrainOutcome::default()
            }
        }
    }
}

#[cfg(test)]
#[path = "spool_drain_tests.rs"]
mod tests;
