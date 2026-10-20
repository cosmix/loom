//! The one resolver of the Claude and Codex install roots, `~/.claude` and
//! `~/.codex`: the only directories Claude Code and Codex read agents, skills
//! and doctrine from. Every runtime reader of an installed asset (hook
//! scripts, skills, the catalog, the doctrine files, Codex hooks) resolves its
//! root here.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::InstallPaths;

/// The loom hooks directory below a Claude root.
const HOOKS_SUBDIR: &str = "hooks/loom";

impl InstallPaths {
    /// `<claude_dir>/hooks/loom`, where loom's Claude Code hook scripts live.
    pub fn claude_hooks_dir(&self) -> PathBuf {
        self.claude_dir.join(HOOKS_SUBDIR)
    }
}

/// Return the Claude and Codex installation directories, `~/.claude` and
/// `~/.codex`.
///
/// No home directory, or a relative one, is an error: a relative root would
/// resolve against each process's working directory, which inside a stage
/// worktree is a path the session writes. Each root is returned canonical
/// (`canonical_or_lexical`), so every consumer spells it as the shell
/// guard's `pwd -P` does, a root not created yet (an install target) included.
pub fn default_paths() -> Result<InstallPaths> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    if !home.is_absolute() {
        bail!(
            "the home directory ($HOME) is the relative path {}: the Claude and Codex roots \
             under it must be absolute, since a relative one resolves against each process's \
             working directory",
            home.display()
        );
    }
    let home = canonical_or_lexical(&home);
    Ok(InstallPaths {
        claude_dir: canonical_or_lexical(&home.join(".claude")),
        codex_dir: canonical_or_lexical(&home.join(".codex")),
    })
}

/// `path` with every symlink resolved when it exists; otherwise its longest
/// existing ancestor resolved and the rest appended as given, so a root not
/// created yet is spelled as it will resolve once it is.
fn canonical_or_lexical(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            canonical_or_lexical(parent).join(name)
        }
        _ => path.to_path_buf(),
    }
}

/// The installed loom hooks directory: [`InstallPaths::claude_hooks_dir`] of
/// [`default_paths`], canonical when it exists; the error says why no
/// directory resolves (no home directory, or a relative one).
pub fn try_claude_hooks_dir() -> Result<PathBuf> {
    default_paths().map(|paths| canonical_or_lexical(&paths.claude_hooks_dir()))
}

/// [`try_claude_hooks_dir`], `None` when it fails.
pub fn claude_hooks_dir() -> Option<PathBuf> {
    try_claude_hooks_dir().ok()
}

/// [`claude_hooks_dir`] as text, for a message or a hook command; the
/// unexpanded default `~/.claude/hooks/loom` when it does not resolve.
pub fn claude_hooks_dir_text() -> String {
    claude_hooks_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|| format!("~/.claude/{HOOKS_SUBDIR}"))
}

#[cfg(test)]
#[path = "tests_resolve.rs"]
mod tests;
