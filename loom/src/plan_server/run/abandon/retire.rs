//! The rename into `doc/plans/archive/`, and finishing an interrupted abandon.

use std::fs;
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{Context, Result};

use crate::fs::safe_fs::{safe_create_dir_all_in_workdir, safe_open_dirfd, safe_rename_in_workdir};
use crate::planning::paths::symlink_component;
use crate::planning::runtime::RuntimePaths;

use super::marker;
use super::plan::{leftover, RunningPlan, Worktrees, ARCHIVE_DIR, PLANS_DIR};
use super::AbandonOutcome;

/// Why the rename of `plan` must not run, or `None` when it may: a symlink on either path,
/// a source that is not a regular file, or a target that exists in any form.
pub(super) fn rename_refusal(root: &Path, plan: &RunningPlan) -> Result<Option<String>> {
    let (source, target) = (plan.source(), plan.retired());
    for rel in [&source, &target] {
        if let Some(link) = symlink_component(root, rel)? {
            let reason = format!(
                "{} is a symbolic link; the plan keeps its name",
                link.display()
            );
            return Ok(Some(reason));
        }
    }
    match fs::symlink_metadata(root.join(&source)) {
        Ok(meta) if meta.file_type().is_file() => {}
        Ok(_) => return Ok(Some(format!("{} is not a regular file", source.display()))),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Some(format!("{} does not exist", source.display())));
        }
        Err(error) => {
            return Err(error).with_context(|| format!("cannot stat {}", source.display()));
        }
    }
    match fs::symlink_metadata(root.join(&target)) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Ok(_) => Ok(Some(format!("{} already exists", target.display()))),
        Err(error) => Err(error).with_context(|| format!("cannot stat {}", target.display())),
    }
}

/// Move `plan` to `doc/plans/archive/RETIRED-PLAN-<slug>.md` without following any link,
/// creating the archive when absent. Commits nothing.
///
/// `renameat` replaces an existing target, so [`rename_refusal`] checks the target is absent
/// immediately before; the window between the two is open only to a process that can already
/// write the repository. Revisit if `safe_fs` gains a no-replace rename.
pub(super) fn rename_to_retired(root: &Path, plan: &RunningPlan) -> Result<AbandonOutcome> {
    let dirfd = match safe_open_dirfd(root) {
        Ok(dirfd) => dirfd,
        Err(error) => {
            let reason =
                format!("the project root cannot be opened without following links: {error:#}");
            return Ok(AbandonOutcome::Refused { reason });
        }
    };
    if let Some(reason) = rename_refusal(root, plan)? {
        return Ok(AbandonOutcome::Refused { reason });
    }
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new(ARCHIVE_DIR), 0o755)?;
    safe_rename_in_workdir(dirfd.as_raw_fd(), &plan.source(), &plan.retired())?;
    Ok(AbandonOutcome::Renamed { to: plan.retired() })
}

/// [`rename_to_retired`] as the last step of a job, its outcome reported as a line. The marker
/// goes only after a rename.
pub(super) fn retire_step(
    root: &Path,
    runtime: &RuntimePaths,
    plan: &RunningPlan,
    on_line: &mut dyn FnMut(&str),
) -> Result<AbandonOutcome> {
    let outcome = match rename_to_retired(root, plan) {
        Ok(outcome) => outcome,
        Err(error) => {
            on_line(&format!("The rename failed: {error:#}"));
            return Err(error);
        }
    };
    match &outcome {
        AbandonOutcome::Renamed { to } => {
            marker::remove_logged(runtime, plan);
            on_line(&format!(
                "Moved {} to {}.",
                plan.source().display(),
                to.display()
            ));
        }
        other => on_line(&format!("The plan keeps its name: {}", describe(other))),
    }
    Ok(outcome)
}

fn describe(outcome: &AbandonOutcome) -> String {
    match outcome {
        AbandonOutcome::Refused { reason } => reason.clone(),
        other => format!("{other:?}"),
    }
}

/// The plan an interrupted abandon left, or why there is none: an abandon marker in `runtime`
/// names an `IN_PROGRESS-` plan under `doc/plans/`, and nothing of the run remains.
pub(super) fn stranded(root: &Path, runtime: &RuntimePaths) -> Result<RunningPlan, String> {
    let plan = marked_plan(root, runtime).ok_or_else(|| {
        "no abandon marker from this server names a running plan; nothing to finish".to_string()
    })?;
    match leftover(root, Worktrees::Any) {
        Some(left) => Err(format!(
            "{left}; finish abandoning renames the plan only once the run is gone"
        )),
        None => Ok(plan),
    }
}

/// The first `IN_PROGRESS-` plan under `doc/plans/`, by file name, that a marker names.
fn marked_plan(root: &Path, runtime: &RuntimePaths) -> Option<RunningPlan> {
    let mut names: Vec<String> = fs::read_dir(root.join(PLANS_DIR))
        .ok()?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|name| RunningPlan::from_rel(&Path::new(PLANS_DIR).join(name)))
        .find(|plan| marker::names(runtime, plan))
}
