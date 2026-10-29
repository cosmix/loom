//! Run state a workspace still holds although its config names no plan.
//!
//! Such a workspace can only be started with `--clean`, which stops the daemon, reaps its
//! sessions and deletes the stage files and the `loom/*` worktrees and branches without the
//! confirmations abandon asks for. The dialog derives `--clean` from the state view, so a
//! workspace holding any of that must not offer it.

use std::fs::{self, DirEntry};
use std::io::{self, ErrorKind};
use std::path::Path;

use crate::fs::work_dir::WorkDir;
use crate::git::list_loom_branches;
use crate::plan_server::wire::run::ChangedPath;

/// The entry that blocks a start for a plan-less workspace holding run state, or `None` when a
/// clean start loses nothing. The view has no refusal field, so the reason travels as a blocking
/// change: the dialog lists it and disables Start, and the start route refuses it with this
/// entry's status (`start::blocked_message`). A check that cannot run counts as run state left.
pub(super) fn blocker(root: &Path) -> anyhow::Result<Option<ChangedPath>> {
    let work_dir = WorkDir::new(root)?;
    Ok(run_state_left(root, &work_dir).map(|left| ChangedPath {
        path: relative_work_dir(root, &work_dir),
        status: format!("{left}; no plan names this run, so run `loom clean --all` in a terminal"),
    }))
}

/// The run-state directory as `root`-relative text: the path `blocker` lists its entry under,
/// derived once here so `start::blocked_message` can tell that entry from a tracked change
/// without reading its prose.
pub(in crate::plan_server::run) fn relative_work_dir(root: &Path, work_dir: &WorkDir) -> String {
    let path = work_dir.root();
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Answers whether a directory holds run state of one kind.
type HoldsRunState = fn(&Path) -> io::Result<bool>;

/// What of a run is left to lose, phrased as a subject ("stage files remain").
fn run_state_left(root: &Path, work_dir: &WorkDir) -> Option<String> {
    let (stages, sessions, worktrees) = (
        work_dir.stages_dir(),
        work_dir.sessions_dir(),
        root.join(".worktrees"),
    );
    let checks: [(&str, &Path, HoldsRunState); 3] = [
        ("stage files remain", stages.as_path(), holds_markdown),
        ("session files remain", sessions.as_path(), holds_markdown),
        ("worktrees remain", worktrees.as_path(), holds_directory),
    ];
    for (left, dir, holds) in checks {
        match holds(dir) {
            Ok(false) => {}
            Ok(true) => return Some(left.to_string()),
            Err(error) => return Some(format!("{} cannot be read ({error})", dir.display())),
        }
    }
    match list_loom_branches(root) {
        Ok(branches) if branches.is_empty() => None,
        Ok(_) => Some("loom/* branches remain".to_string()),
        Err(error) => Some(format!("the loom/* branches cannot be listed ({error:#})")),
    }
}

/// Whether `dir` holds a `*.md` file. An entry that cannot be read propagates, so the check
/// fails closed.
fn holds_markdown(dir: &Path) -> io::Result<bool> {
    any_entry(dir, is_markdown)
}

fn holds_directory(dir: &Path) -> io::Result<bool> {
    any_entry(dir, is_dir)
}

fn any_entry(dir: &Path, wanted: fn(&DirEntry) -> bool) -> io::Result<bool> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    for entry in entries {
        if wanted(&entry?) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_markdown(entry: &DirEntry) -> bool {
    entry.path().extension().is_some_and(|ext| ext == "md")
}

fn is_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir())
}
