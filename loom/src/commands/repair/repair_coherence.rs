//! `loom repair` check for a stage that claims `Executing` but does not name
//! a live worker session of its own kind — the same condition the per-tick
//! watchdog in `orchestrator::core::coherence` repairs automatically. This is
//! the post-hoc audit for a daemon that was not running when it happened, or
//! for confirming the watchdog caught everything.

use std::path::Path;

use anyhow::{Context, Result};

use super::{RepairIssue, Severity};
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::coherence::{
    block_incoherent_stage, executing_stage_incoherence, load_assigned_session,
};
use crate::verify::transitions::list_all_stages;

/// Resolve the state directory for `repo_root` the same way `repair.rs`'s
/// other checks do: `WorkDir` when it can be determined, falling back to the
/// nested `.loom/work` layout otherwise. Never assumes the legacy `.work`
/// path — the state root moved to `.loom/work`.
fn resolve_work_dir(repo_root: &Path) -> std::path::PathBuf {
    WorkDir::new(repo_root)
        .map(|wd| wd.root().to_path_buf())
        .unwrap_or_else(|_| repo_root.join(".loom").join("work"))
}

/// An `Executing` stage that does not name a live worker session of its own
/// kind, for every stage under `repo_root`'s state directory. Mirrors the
/// "Phantom merge" check in `repair.rs`: an unreadable stages directory
/// produces one INFO issue rather than failing the whole repair run.
pub(super) fn check_incoherent_executing_stages(repo_root: &Path) -> Vec<RepairIssue> {
    let work_dir = resolve_work_dir(repo_root);
    if !work_dir.is_dir() {
        return Vec::new();
    }
    let stages = match list_all_stages(&work_dir) {
        Ok(stages) => stages,
        Err(_) => {
            return vec![RepairIssue {
                severity: Severity::Info,
                description:
                    "Could not audit executing stages for coherence (stages directory unreadable)"
                        .to_string(),
                fix_description: "Investigate .work/stages/ directory manually".to_string(),
            }];
        }
    };

    stages
        .iter()
        .filter_map(|stage| {
            let assigned = load_assigned_session(&work_dir, stage).unwrap_or(None);
            let reason = executing_stage_incoherence(stage, assigned.as_ref())?;
            Some(RepairIssue {
                severity: Severity::Critical,
                description: format!("Incoherent executing stage '{}': {reason}", stage.id),
                fix_description: "Mark the stage Blocked (infrastructure) and clear its session \
                                   pointer, then `loom stage retry <id>`"
                    .to_string(),
            })
        })
        .collect()
}

/// Apply the fix for [`check_incoherent_executing_stages`]: escalate the
/// named stage to Blocked with its session pointer cleared.
pub(super) fn fix_incoherent_executing_stage(repo_root: &Path, description: &str) -> Result<bool> {
    let rest = description
        .strip_prefix("Incoherent executing stage '")
        .with_context(|| format!("Cannot parse stage ID from: {description}"))?;
    let (stage_id, reason) = rest
        .split_once("': ")
        .with_context(|| format!("Cannot parse stage ID from: {description}"))?;

    let work_dir = resolve_work_dir(repo_root);
    let fixed = block_incoherent_stage(&work_dir, stage_id, reason)?;
    Ok(fixed.is_some())
}
