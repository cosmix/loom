//! The write denies every session capsule carries
//! (`doc/plans/PLAN-loom-state-confinement.md`, sections 10 and 11), in both
//! layers: a `sandbox.filesystem.denyWrite` path binds Bash and everything it
//! runs, and a `permissions.deny` `Edit` rule binds the native file tools,
//! which do not run under the OS sandbox. A deny wins over any allow in both.
//!
//! Every entry is a literal path, and an `Edit` rule may add a trailing `/**`
//! to a literal directory: any other wildcard risks a recursive expansion, so
//! a path no rule can name literally refuses the spawn.

use anyhow::{bail, Context, Result};
use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use super::{push_unique, surface_path, GLOB_CHARS, HOME_SURFACES};
use crate::codex::CODEX_PLUGIN_DATA_GRANT;

/// `~/.claude/plugins`, relative to the home directory.
const PLUGINS_DIR: &str = ".claude/plugins";

/// Where a session runs, and what else its capsule denies.
pub(crate) struct DenyInputs<'a> {
    /// The canonical repository root.
    pub repo_root: &'a Path,
    /// The canonical state root. A `.work` directly under the repository root
    /// is the legacy layout, whose `.work` spellings are denied too.
    pub state_root: &'a Path,
    /// The stage worktree the session runs in; `None` when it runs in the
    /// checkout.
    pub worktree: Option<&'a Path>,
    /// Every directory holding an executable loom's hooks run.
    pub executable_dirs: &'a [PathBuf],
    /// The codex lane's `~/.claude/plugins` entries (`codex_plugin_entries`);
    /// `None` denies the whole directory.
    pub plugin_entries: Option<&'a [String]>,
    /// Every root a session could write
    /// (`sandbox::control_surfaces::session_writable_roots`, threaded through
    /// as `HostFacts::writable_roots`): the scratch root, each stage's
    /// `allowWrite` grants, and the package-manager and codex caches. An
    /// `executable_dirs` entry that is an ancestor of one is skipped, on top
    /// of `repo_root` and `worktree` below.
    pub writable_roots: &'a [PathBuf],
}

/// Both deny layers of one capsule.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SessionDenies {
    /// `sandbox.filesystem.denyWrite` paths.
    pub deny_write: Vec<String>,
    /// `permissions.deny` `Edit` rules.
    pub edit: Vec<String>,
}

/// The capsule's write denies: the state root (`.loom`, and `.work` on the
/// legacy layout), every section 11 control surface, and by location the
/// worktree's own `.loom` and `.claude`, or from the checkout its
/// `.worktrees` and `.claude`.
///
/// An `executable_dirs` entry that is an ancestor of, or equal to, the
/// repository root, the stage worktree, or any other session-writable root
/// (`is_ancestor_of_writable_root`) is skipped: denying it would deny writes
/// across its entire subtree, including the writable root itself, and it
/// needs no deny anyway — a write outside the session's grants is already
/// refused by the sandbox's `allowOnly` default.
pub(crate) fn session_denies(inputs: &DenyInputs<'_>) -> Result<SessionDenies> {
    let repo = inputs.repo_root;
    let mut denies = SessionDenies::default();
    denies.absolute(&repo.join(".loom"), true)?;
    denies.relative(".loom");
    if inputs.state_root == repo.join(".work") {
        denies.absolute(inputs.state_root, true)?;
        denies.relative(".work");
    }
    for surface in HOME_SURFACES {
        denies.home(surface);
    }
    let whole_plugins_dir = [format!("{PLUGINS_DIR}/**")];
    for surface in inputs.plugin_entries.unwrap_or(&whole_plugins_dir[..]) {
        denies.home(surface);
    }
    denies.absolute(&repo.join(".git").join("hooks"), true)?;
    denies.absolute(&repo.join(".git").join("config"), false)?;
    for dir in inputs.executable_dirs {
        if is_ancestor_of_writable_root(dir, repo, inputs.worktree, inputs.writable_roots) {
            continue;
        }
        denies.absolute(dir, true)?;
    }
    match inputs.worktree {
        Some(worktree) => {
            // The relative `Edit(.loom/**)` above already binds the file tools here.
            let own_state = worktree.join(".loom");
            push_unique(&mut denies.deny_write, literal(&own_state)?.to_string());
            denies.absolute(&worktree.join(".claude"), true)?;
            denies.relative(".claude");
        }
        None => {
            for dir in [".worktrees", ".claude"] {
                denies.absolute(&repo.join(dir), true)?;
                denies.relative(dir);
            }
        }
    }
    Ok(denies)
}

impl SessionDenies {
    /// Deny the absolute `path` in both layers; a `dir` denies what it holds.
    /// A permission rule takes `//` for an absolute path (a single `/` is
    /// project-relative there); the OS sandbox takes the plain path.
    fn absolute(&mut self, path: &Path, dir: bool) -> Result<()> {
        let path = literal(path)?;
        push_unique(&mut self.deny_write, path.to_string());
        push_unique(&mut self.edit, edit_rule(&format!("/{path}"), dir));
        Ok(())
    }

    /// Deny a home-relative surface in rule form in both layers, spelled `~/`.
    fn home(&mut self, surface: &str) {
        let (path, dir) = surface_path(surface);
        push_unique(&mut self.deny_write, format!("~/{path}"));
        push_unique(&mut self.edit, edit_rule(&format!("~/{path}"), dir));
    }

    /// Deny the project-relative directory `dir` to the file tools; the OS
    /// sandbox gets its absolute spelling instead.
    fn relative(&mut self, dir: &str) {
        push_unique(&mut self.edit, edit_rule(dir, true));
    }
}

/// Whether `dir` is an ancestor of, or equal to, a root the session may
/// already write: the repository root, the worktree (inside one), or any
/// other entry of `writable_roots` (the scratch root, an `allowWrite` grant,
/// or a package-manager/codex cache). A `LOOM_HOOK_PATH` entry lands here on
/// nothing more than a PATH coincidence — e.g. `~/src` on `PATH` with the
/// repo checked out at `~/src/loom`, or `~/.bun` on `PATH` with a granted
/// `~/.bun/install/cache` — and denying it would deny writes across
/// everything below it, the writable root included, rather than just the
/// executable it holds.
///
/// `repo_root` and `worktree` are compared as given, matching the historical
/// behaviour; `writable_roots` and `dir` are compared canonicalized where
/// they resolve on disk, since a root can reach the same directory through a
/// different spelling (macOS symlinks `/tmp` to `/private/tmp`).
fn is_ancestor_of_writable_root(
    dir: &Path,
    repo_root: &Path,
    worktree: Option<&Path>,
    writable_roots: &[PathBuf],
) -> bool {
    if repo_root.starts_with(dir) || worktree.is_some_and(|worktree| worktree.starts_with(dir)) {
        return true;
    }
    let dir = canonical_or_self(dir);
    writable_roots
        .iter()
        .any(|root| canonical_or_self(root).starts_with(&dir))
}

/// `path` canonicalized when it resolves on disk, unchanged otherwise: a
/// literal fallback lets a fixture path that exists only in a test compare
/// correctly, while a real path compares by what it actually resolves to.
fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn edit_rule(path: &str, dir: bool) -> String {
    if dir {
        format!("Edit({path}/**)")
    } else {
        format!("Edit({path})")
    }
}

/// `path` as text a deny can name literally: valid UTF-8, no glob character.
fn literal(path: &Path) -> Result<&str> {
    let text = path.to_str().with_context(|| {
        format!(
            "cannot deny writing {}: the path is not valid UTF-8",
            path.display()
        )
    })?;
    if text.contains(GLOB_CHARS) {
        bail!(
            "cannot deny writing {text}: it holds a glob character, so no rule names it literally"
        );
    }
    Ok(text)
}

/// The codex lane's `~/.claude/plugins` denies, read from `home` at spawn:
/// each entry on the way down to `CODEX_PLUGIN_DATA_GRANT` except the one the
/// way continues through, so the grant stays writable and nothing beside it
/// is. Each is home-relative, in rule form, sorted; a missing directory lists
/// nothing. An entry created after the spawn is not listed, but the OS
/// sandbox grants nothing there to create it with.
pub(crate) fn codex_plugin_entries(home: &Path) -> Result<Vec<String>> {
    let grant = CODEX_PLUGIN_DATA_GRANT
        .strip_prefix("~/")
        .and_then(|path| Path::new(path).strip_prefix(PLUGINS_DIR).ok())
        .expect("CODEX_PLUGIN_DATA_GRANT lies under ~/.claude/plugins");
    let mut entries = Vec::new();
    let mut dir = PathBuf::from(PLUGINS_DIR);
    for keep in grant.components() {
        for entry in list_dir(&home.join(&dir))? {
            if entry.file_name() == keep.as_os_str() {
                continue;
            }
            let relative = dir.join(entry.file_name());
            let text = literal(&relative)?;
            entries.push(if entry.path().is_dir() {
                format!("{text}/**")
            } else {
                text.to_string()
            });
        }
        dir.push(keep);
    }
    entries.sort();
    Ok(entries)
}

/// The entries of `dir`; none when it does not exist.
fn list_dir(dir: &Path) -> Result<Vec<DirEntry>> {
    let listed = match std::fs::read_dir(dir) {
        Ok(entries) => entries.collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    };
    listed.with_context(|| format!("cannot list {}", dir.display()))
}

#[cfg(test)]
#[path = "tests_session_denies.rs"]
mod tests;
