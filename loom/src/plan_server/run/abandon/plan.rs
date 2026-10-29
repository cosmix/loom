//! The plan an abandon acts on, and what of a run remains on disk.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::commands::common::resolve_state_dir;
use crate::fs::plan_lifecycle::{IN_PROGRESS_PREFIX, RETIRED_PREFIX};
use crate::git::branch::list_loom_branches;
use crate::git::worktree::list_worktrees;
use crate::planning::slug::{plan_slug, plan_state, PlanState};
use crate::verify::transitions::list_all_stages;

/// Where plans live, and where retired plans go.
pub(super) const PLANS_DIR: &str = "doc/plans";
pub(super) const ARCHIVE_DIR: &str = "doc/plans/archive";

/// A running plan, named by its slug: every path is rebuilt from the slug, never taken from
/// the workspace configuration as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningPlan {
    pub slug: String,
}

impl RunningPlan {
    /// `doc/plans/IN_PROGRESS-PLAN-<slug>.md`.
    pub fn source(&self) -> PathBuf {
        Path::new(PLANS_DIR).join(format!("{IN_PROGRESS_PREFIX}PLAN-{}.md", self.slug))
    }

    /// `doc/plans/archive/RETIRED-PLAN-<slug>.md`.
    pub fn retired(&self) -> PathBuf {
        Path::new(ARCHIVE_DIR).join(format!("{RETIRED_PREFIX}PLAN-{}.md", self.slug))
    }

    pub fn source_string(&self) -> String {
        self.source().to_string_lossy().into_owned()
    }

    /// The plan `rel` names when it is a running plan directly under `doc/plans/`.
    pub fn from_rel(rel: &Path) -> Option<RunningPlan> {
        if plan_state(rel) != Some(PlanState::Running) {
            return None;
        }
        let slug = plan_slug(rel.file_name()?.to_str()?)?;
        let plan = RunningPlan { slug };
        (plan.source() == rel).then_some(plan)
    }
}

/// The workspace's plan when it is `IN_PROGRESS-` (the workspace is running or stopped).
pub(super) fn running_plan(root: &Path) -> Option<RunningPlan> {
    let state_dir = resolve_state_dir(root);
    let source = crate::fs::get_source_path(&state_dir).ok()??;
    let rel = if source.is_absolute() {
        let canonical = root.canonicalize().ok()?;
        source
            .strip_prefix(root)
            .or_else(|_| source.strip_prefix(&canonical))
            .ok()?
            .to_path_buf()
    } else {
        source
    };
    RunningPlan::from_rel(&rel)
}

/// The run's stage ids, read before `loom clean` deletes the workspace; empty when unreadable
/// (the worktree check then relies on `git worktree list` alone).
pub(super) fn run_stage_ids(root: &Path) -> Vec<String> {
    list_all_stages(&resolve_state_dir(root))
        .map(|stages| stages.into_iter().map(|stage| stage.id).collect())
        .unwrap_or_default()
}

/// Which worktrees count as the run's.
pub(super) enum Worktrees<'a> {
    /// Every directory under `.worktrees/`.
    Any,
    /// Directories `git worktree list` lists, or named after one of these stage ids.
    OfRun(&'a [String]),
}

/// What of a run remains: the workspace, one of its worktrees, or a `loom/*` branch. A check
/// that cannot run counts as something remaining.
pub(super) fn leftover(root: &Path, worktrees: Worktrees<'_>) -> Option<String> {
    let state_dir = resolve_state_dir(root);
    match fs::symlink_metadata(&state_dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Ok(_) => return Some(format!("{} remains", shown(root, &state_dir))),
        Err(error) => {
            return Some(format!(
                "{} cannot be checked: {error}",
                state_dir.display()
            ))
        }
    }
    match worktree_left(root, &worktrees) {
        Ok(None) => {}
        Ok(Some(dir)) => return Some(format!("the worktree {dir} remains")),
        Err(error) => return Some(format!("the worktrees cannot be checked: {error:#}")),
    }
    match list_loom_branches(root) {
        Ok(branches) if branches.is_empty() => None,
        Ok(branches) => Some(format!("the branch {} remains", branches.join(", "))),
        Err(error) => Some(format!("the loom/* branches cannot be listed: {error:#}")),
    }
}

fn shown(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The first `.worktrees/<name>` directory that counts as the run's.
fn worktree_left(root: &Path, worktrees: &Worktrees<'_>) -> Result<Option<String>> {
    let entries = match fs::read_dir(root.join(".worktrees")) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(entries) => entries,
    };
    let listed: Vec<PathBuf> = match worktrees {
        Worktrees::Any => Vec::new(),
        Worktrees::OfRun(_) => list_worktrees(root)?
            .into_iter()
            .filter_map(|worktree| worktree.path.canonicalize().ok())
            .collect(),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let counts = match worktrees {
            Worktrees::Any => true,
            Worktrees::OfRun(stage_ids) => {
                stage_ids.contains(&name)
                    || entry
                        .path()
                        .canonicalize()
                        .is_ok_and(|path| listed.contains(&path))
            }
        };
        if counts {
            return Ok(Some(format!(".worktrees/{name}")));
        }
    }
    Ok(None)
}
