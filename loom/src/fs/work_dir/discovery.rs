//! Free functions behind `WorkDir::resolve`: locating a workspace at a given
//! directory, recognizing a state-root-shaped hint, bounding the upward walk
//! at the nearest enclosing repository, and converting a resolved root back
//! to its project root.

use std::path::{Path, PathBuf};

use super::{Layout, LEGACY_WORK_DIR, LOOM_DIR, WORK_DIR};

/// The workspace rooted at `dir`, if either layout has a `config.toml` there.
///
/// Keyed on the config FILE, never on directory existence: `~/.loom/config.toml`
/// is a user-level file and `.loom/cache/` appears in any project that has run
/// `loom map`, so a bare `.loom/` marks nothing. Nested wins over legacy when
/// both are present.
pub(super) fn workspace_at(dir: &Path) -> Option<(PathBuf, Layout)> {
    let nested = dir.join(LOOM_DIR).join(WORK_DIR);
    if nested.join("config.toml").exists() {
        return Some((nested, Layout::Nested));
    }
    let legacy = dir.join(LEGACY_WORK_DIR);
    if legacy.join("config.toml").exists() {
        return Some((legacy, Layout::Legacy));
    }
    None
}

/// The layout `base` names when it already IS a state root rather than a
/// project root, in either spelling.
///
/// Hook entry points (see `commands/hook/reconcile_graph.rs`) hand `WorkDir::new`
/// `LOOM_WORK_DIR`, which names the state directory ITSELF, not its parent — so
/// a `base` that already names one must resolve to itself rather than get a
/// second state root appended under it. Both spellings need recognising: after
/// the move the pinned value ends `.loom/work`, whose final component alone is
/// the unremarkable `work`, while a workspace created before the move still
/// pins a single `.work`. Miss either and a stale pin materializes a phantom
/// `<...>/.loom/work/.loom/work` (or `<...>/.work/.work`), whose `repo_root()`
/// is the state directory itself. `initialize()` creates the root this returns,
/// so the branch keeps that creation correct for a state-root-named hint too.
pub(super) fn base_names_state_root(base: &Path) -> Option<Layout> {
    let name = base.file_name()?;
    if name == std::ffi::OsStr::new(WORK_DIR)
        && base.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new(LOOM_DIR))
    {
        return Some(Layout::Nested);
    }
    if name == std::ffi::OsStr::new(LEGACY_WORK_DIR) {
        return Some(Layout::Legacy);
    }
    None
}

/// The nearest ancestor of `dir` (inclusive) holding a real `.git` entry, or
/// `None` when there is none.
///
/// This is the bound on the upward workspace search: a `.git` marks the one
/// tree whose `config.toml` can legitimately be this base's workspace. A
/// `.git` FILE (a worktree's pointer to the main repo's gitdir) counts on
/// existence; a `.git` DIRECTORY counts only when it holds `HEAD` — a stray
/// empty directory merely named `.git` cannot masquerade as a repo boundary.
pub(super) fn nearest_git_root(dir: &Path) -> Option<&Path> {
    let mut current = dir;
    loop {
        if crate::fs::git_marker::is_real_git_dir(&current.join(".git")) {
            return Some(current);
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return None,
        }
    }
}

/// Apply the layout's hop count to a state-root path.
pub(super) fn repo_root_of(root: &Path, layout: Layout) -> Option<&Path> {
    match layout {
        Layout::Nested => root.parent()?.parent(),
        Layout::Legacy => root.parent(),
    }
}

/// The upward search `WorkDir::resolve` runs once neither `base` itself nor a
/// state-root-shaped hint answers: walk from `abs` toward `repo_root`
/// (inclusive), stopping at `floor` if given, returning the first workspace
/// found.
///
/// `floor` is the OS temp root when `abs` sits under it — a `TMPDIR` nested
/// inside a git checkout (e.g. a test suite pointed at `<repo>/target/tmp`)
/// sits BELOW that checkout's `.git`, so the repo-root bound alone would walk
/// straight into the checkout's live `.loom/work`. See `WorkDir::resolve`'s
/// own comment for the incident this guards against.
pub(super) fn walk_up(
    abs: &Path,
    repo_root: &Path,
    floor: Option<&Path>,
) -> Option<(PathBuf, Layout)> {
    let mut current = abs;
    loop {
        if floor.is_some_and(|floor| current == floor) {
            return None;
        }
        if let Some(found) = workspace_at(current) {
            return Some(found);
        }
        if current == repo_root {
            return None;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return None,
        }
    }
}
