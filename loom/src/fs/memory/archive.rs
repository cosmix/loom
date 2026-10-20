//! Durable run-state archiving before the transient work directory is removed.

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

use crate::context::delivery::plan_key_from;
use crate::fs::work_dir::WorkDir;

/// Copy `<work_dir>/memory/` and `<work_dir>/telemetry/` (when present) to
/// `<main_root>/.loom/memory/archive/<plan-id-or-default>-<YYYYmmddTHHMMSSZ>/`.
///
/// Returns `Ok(Some(path))` on success and `Ok(None)` when there was nothing
/// to archive. When archiving fails and `<work_dir>/memory` holds a `*.md`
/// journal, the error is returned (it names the archive path) so callers keep
/// the state. A failure with telemetry only prints a warning and returns
/// `Ok(None)`.
pub fn archive_run_state(
    work_dir: &Path,
    main_root: &Path,
    plan_id: Option<&str>,
) -> Result<Option<PathBuf>> {
    match try_archive_run_state(work_dir, main_root, plan_id) {
        Ok(path) => Ok(path),
        Err(error) if has_journal(work_dir) => Err(error),
        Err(error) => {
            eprintln!("Warning: Failed to archive run state: {error:#}");
            Ok(None)
        }
    }
}

/// Archive the state directory `work_dir` ahead of its removal, reading the plan
/// id from its config and archiving under the main project root it resolves to
/// (`repo_root` when it is not a workspace).
///
/// Callers must propagate the error with `?` before deleting `work_dir`: an
/// `Err` means a journal could not be archived and the state must stay.
pub fn archive_state_dir(work_dir: &Path, repo_root: &Path) -> Result<Option<PathBuf>> {
    let workspace = WorkDir::new(work_dir).ok();
    let plan_id = workspace
        .as_ref()
        .and_then(|workspace| workspace.load_config().ok().flatten())
        .and_then(|config| config.plan_id().map(str::to_owned));
    let main_root = workspace
        .and_then(|workspace| workspace.main_project_root())
        .unwrap_or_else(|| repo_root.to_path_buf());
    archive_run_state(work_dir, &main_root, plan_id.as_deref())
}

/// True when `<work_dir>/memory` holds a `*.md` journal. An entry that exists
/// but cannot be listed (permissions, not a directory) counts as holding one,
/// so the archive error is returned instead of being demoted to a warning.
fn has_journal(work_dir: &Path) -> bool {
    let memory = work_dir.join("memory");
    match fs::read_dir(&memory) {
        Ok(entries) => entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "md")),
        Err(_) => memory.symlink_metadata().is_ok(),
    }
}

fn try_archive_run_state(
    work_dir: &Path,
    main_root: &Path,
    plan_id: Option<&str>,
) -> Result<Option<PathBuf>> {
    let source_names: Vec<_> = ["memory", "telemetry"]
        .into_iter()
        .filter(|name| work_dir.join(name).exists())
        .collect();
    if source_names.is_empty() {
        return Ok(None);
    }

    let archive_name = format!(
        "{}-{}",
        plan_key_from(plan_id),
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let archive_path = main_root.join(".loom/memory/archive").join(archive_name);
    fs::create_dir_all(&archive_path)
        .with_context(|| format!("creating archive directory {}", archive_path.display()))?;

    for name in source_names {
        copy_path(&work_dir.join(name), &archive_path.join(name))
            .with_context(|| format!("archiving run state to {}", archive_path.display()))?;
    }

    Ok(Some(archive_path))
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)
            .with_context(|| format!("creating archive directory {}", destination.display()))?;
        for entry in fs::read_dir(source)
            .with_context(|| format!("reading archive source {}", source.display()))?
        {
            let entry =
                entry.with_context(|| format!("reading archive source {}", source.display()))?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }

    fs::copy(source, destination).with_context(|| {
        format!(
            "copying archive source {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
