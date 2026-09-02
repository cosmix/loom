//! `loom context record-edit` — remember which files a stage's agents edited.
//!
//! Runs from a PostToolUse hook on every Write/Edit/MultiEdit/NotebookEdit
//! call, which sets the whole design: it must be fast, silent, and incapable
//! of failing the tool call that invoked it. It records **paths only** —
//! never tool results, never diffs, never file contents. That is a privacy
//! and blast-radius boundary: whatever can read a stage overlay can read this
//! file, so the payload is kept to the names of files that changed.
//!
//! The record exists so a future incremental context-overlay refresh could
//! invalidate exactly the entries a stage touched, instead of recomputing the
//! whole overlay from scratch. No such refresh reads this file yet: as of
//! this writing, `dirty-paths.json` is written on every edit and consumed by
//! nothing — it is pure input for a consumer that has not been built.

use crate::context::graph_store::GraphStore;
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::work_dir::WorkDir;
use crate::validation::validate_id;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The stage's recorded edits, stored beside its context overlay.
const DIRTY_PATHS_FILE: &str = "dirty-paths.json";

/// Path prefixes that hold loom's own state, its derived data, or generated
/// knowledge. An agent edit landing in one of these is machinery, not work.
///
/// Deliberately NOT the source-graph walker's `EXCLUDED_ROOTS`: that walk always
/// starts at a project root and so can treat `.worktrees` as machinery
/// unconditionally, while this one runs from inside a worktree where every file
/// legitimately sits below it. Merging the two lists would silently discard
/// every edit an orchestrated stage makes.
const IGNORED_PREFIXES: &[&str] = &[".loom", ".work", "doc/loom/knowledge"];

/// Directory names that hold dependencies or build output at any depth.
const IGNORED_DIRECTORIES: &[&str] =
    &["target", "node_modules", "vendor", "dist", "build", ".venv"];

/// Where the orchestrator parks per-stage worktrees.
const WORKTREES_DIR: &str = ".worktrees";

/// A stage's edited paths — and nothing else.
///
/// The absence of a content field is the point; see the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DirtyPaths {
    /// Checkout-relative paths, de-duplicated and sorted.
    #[serde(default)]
    paths: Vec<String>,
    /// When the most recent edit was folded in.
    recorded_at: DateTime<Utc>,
}

/// Record the paths an agent just edited against `stage`'s context overlay.
///
/// Prints nothing on the happy path. Every recoverable condition — an unknown
/// stage, no active plan, an unreadable directory, a path outside the checkout,
/// an empty path list — returns `Ok(())`, because a hook that fails an edit is
/// far worse than a hook that records nothing. Only a malformed invocation,
/// such as an empty or unsafe `--stage`, is an error.
pub fn record_edit(stage: &str, paths: &[PathBuf]) -> Result<()> {
    validate_id(stage).context("Invalid --stage value")?;
    if paths.is_empty() {
        return Ok(());
    }

    if let Err(error) = try_record(stage, paths) {
        tracing::debug!(stage, %error, "Skipped recording an edit");
    }
    Ok(())
}

/// The fallible half of [`record_edit`], so the happy path can use `?` freely.
/// Its caller swallows every error it returns.
fn try_record(stage: &str, paths: &[PathBuf]) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let root = checkout_root(&work_dir)?;
    let plan = active_plan(&work_dir)?;
    let scanning_main = is_main_checkout(&work_dir, &root);

    let recorded: BTreeSet<String> = paths
        .iter()
        .filter_map(|path| relative_to_root(&root, path, scanning_main))
        .collect();
    if recorded.is_empty() {
        return Ok(());
    }

    // `overlay_dir` reads only the work root, so the cache root handed to the
    // constructor never reaches the returned path — the same construction
    // `crate::context::delivery` uses to resolve a stage's overlay.
    let overlay_dir = GraphStore::new(work_dir.root(), work_dir.root()).overlay_dir(&plan, stage);
    merge_dirty_paths(&overlay_dir, recorded)
}

/// The root of the checkout this command runs in — the worktree when inside
/// one, the main checkout otherwise.
///
/// Payload paths are normalized against THIS root and never against the main
/// checkout: an orchestrated stage's files physically live below
/// `.worktrees/<stage>/`, and measuring them from the main checkout would make
/// every one of them look administrative.
fn checkout_root(work_dir: &WorkDir) -> Result<PathBuf> {
    let root = work_dir
        .project_root()
        .context("Could not determine the project root")?;
    absolutize(root)
}

/// The active plan id, which names the overlay directory this edit belongs to.
fn active_plan(work_dir: &WorkDir) -> Result<String> {
    work_dir
        .load_config()?
        .and_then(|config| config.plan_id().map(str::to_string))
        .context("No active plan to record the edit against")
}

/// True when `root` is the main checkout rather than a linked worktree.
///
/// This is the predicate that decides whether a `.worktrees/...` path is
/// administrative. When the main root cannot be resolved the answer is `false`:
/// recording a path that could have been skipped is harmless, silently dropping
/// every edit an orchestrated stage makes is not.
fn is_main_checkout(work_dir: &WorkDir, root: &Path) -> bool {
    work_dir
        .main_project_root()
        .and_then(|main| absolutize(&main).ok())
        .is_some_and(|main| main == root)
}

/// Make `path` absolute against the current directory and resolve `.`/`..`
/// lexically. Touches the filesystem only to read the current directory.
fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(lexical_normalize(path));
    }
    let cwd = std::env::current_dir().context("Could not read the current directory")?;
    Ok(lexical_normalize(&cwd.join(path)))
}

/// Normalize one payload path to a root-relative string, or drop it.
///
/// Drops anything not worth recording and anything that does not provably stay
/// inside `root`: a `..` hop out, or a path reached through a symlinked
/// directory. Escapes are rejected, never clamped.
fn relative_to_root(root: &Path, path: &Path, scanning_main: bool) -> Option<String> {
    let absolute = if path.is_absolute() {
        lexical_normalize(path)
    } else {
        lexical_normalize(&root.join(path))
    };

    let relative = absolute.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty()
        || is_ignored(relative, scanning_main)
        || has_symlink_component(root, relative)
    {
        return None;
    }
    Some(relative.to_str()?.to_string())
}

/// True when `relative` names something no stage should record.
///
/// `.worktrees/` counts as machinery ONLY when scanning the main checkout.
/// Inside a worktree every file legitimately sits below `.worktrees/<stage>/`,
/// so skipping it there would discard the stage's entire diff.
fn is_ignored(relative: &Path, scanning_main: bool) -> bool {
    if scanning_main && relative.starts_with(WORKTREES_DIR) {
        return true;
    }
    if IGNORED_PREFIXES
        .iter()
        .any(|prefix| relative.starts_with(prefix))
    {
        return true;
    }
    relative.components().any(|component| match component {
        Component::Normal(name) => name
            .to_str()
            .is_some_and(|name| IGNORED_DIRECTORIES.contains(&name)),
        _ => false,
    })
}

/// True when any existing component of `relative`, walked from `root` down, is
/// a symlink. A missing component is fine — a freshly created file has none.
///
/// `std::fs::canonicalize` would answer this too, but it follows symlinks
/// instead of reporting them, and resolves the whole path on what is a hot
/// per-edit path.
fn has_symlink_component(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// Resolve `.` and `..` textually, without touching the filesystem.
///
/// An unresolvable `..` is kept verbatim so the result still fails the
/// containment check instead of silently clamping to the root.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// Fold `recorded` into the stage's dirty-path set, under the stage
/// directory's lock and through a crash-atomic replace.
///
/// Merging rather than overwriting is what makes a per-edit hook additive: each
/// invocation sees one edit and must not erase the ones before it.
fn merge_dirty_paths(overlay_dir: &Path, recorded: BTreeSet<String>) -> Result<()> {
    let path = overlay_dir.join(DIRTY_PATHS_FILE);
    locked_dir_update(overlay_dir, || {
        let mut merged = read_dirty_paths(&path);
        merged.extend(recorded);
        let document = DirtyPaths {
            paths: merged.into_iter().collect(),
            recorded_at: Utc::now(),
        };
        let json = serde_json::to_string_pretty(&document)
            .context("Failed to serialize the recorded edit paths")?;
        atomic_write_locked(&path, &json)
    })
}

/// The paths already recorded for this stage. An absent, unreadable, or
/// malformed file reads as "nothing recorded yet" — the record is an
/// optimisation, and refusing to write over a broken file would keep it broken.
fn read_dirty_paths(path: &Path) -> BTreeSet<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    serde_json::from_str::<DirtyPaths>(&raw)
        .map(|document| document.paths.into_iter().collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tests_record_edit.rs"]
mod tests;
