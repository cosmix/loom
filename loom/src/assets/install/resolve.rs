//! The one resolver of the Claude and Codex install roots. Every runtime
//! reader of an installed asset (hook scripts, skills, the catalog, the
//! doctrine files, Codex hooks) resolves its root here, so a relocated install
//! is found by the same rule that placed it.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::{roots, InstallPaths};

/// The loom hooks directory below a Claude root.
const HOOKS_SUBDIR: &str = "hooks/loom";

/// The Claude root override.
pub(crate) const CLAUDE_OVERRIDE: &str = "LOOM_CLAUDECODE_INSTALL_DIR";
/// The Codex root override.
pub(crate) const CODEX_OVERRIDE: &str = "LOOM_CODEX_INSTALL_DIR";

impl InstallPaths {
    /// `<claude_dir>/hooks/loom`, where loom's Claude Code hook scripts live.
    pub fn claude_hooks_dir(&self) -> PathBuf {
        self.claude_dir.join(HOOKS_SUBDIR)
    }
}

/// Return the Claude and Codex installation directories.
///
/// Each directory is resolved independently: `LOOM_CLAUDECODE_INSTALL_DIR` /
/// `LOOM_CODEX_INSTALL_DIR` wins when set to a nonempty path; otherwise the
/// root recorded by the last bare install (`record_roots`, at
/// `<home>/.config/loom/install-roots.toml`) wins; otherwise the default,
/// `~/.claude` and `~/.codex`.
///
/// A relative root is an error naming its source: it would resolve against
/// each process's working directory, which inside a stage worktree is a path
/// the session writes. Each root is returned canonical
/// (`canonical_or_lexical`), so every consumer spells it as the shell
/// guard's `pwd -P` does, a root not created yet (an install target) included.
pub fn default_paths() -> Result<InstallPaths> {
    let claude_env = env_root(CLAUDE_OVERRIDE)?;
    let codex_env = env_root(CODEX_OVERRIDE)?;
    let (claude_dir, codex_dir) = match (claude_env, codex_env) {
        (Some(claude_dir), Some(codex_dir)) => (claude_dir, codex_dir),
        (claude_env, codex_env) => {
            let home = dirs::home_dir().context("Failed to determine home directory")?;
            let home = absolute_root(home, "the home directory ($HOME)")?;
            let (recorded_claude, recorded_codex) = roots::read(&home);
            let record = |key: &str| {
                format!(
                    "the {key} entry of {}",
                    home.join(roots::RECORD_PATH).display()
                )
            };
            let claude_dir = match (claude_env, recorded_claude) {
                (Some(dir), _) => dir,
                (None, Some(dir)) => absolute_root(dir, &record("claude_dir"))?,
                (None, None) => canonical_or_lexical(&home.join(".claude")),
            };
            let codex_dir = match (codex_env, recorded_codex) {
                (Some(dir), _) => dir,
                (None, Some(dir)) => absolute_root(dir, &record("codex_dir"))?,
                (None, None) => canonical_or_lexical(&home.join(".codex")),
            };
            (claude_dir, codex_dir)
        }
    };
    Ok(InstallPaths {
        claude_dir,
        codex_dir,
    })
}

/// Persist `paths` as the roots a future bare install (no env override, no
/// explicit `--claude-dir`/`--codex-dir`) should resolve to, instead of the
/// `~/.claude` / `~/.codex` defaults.
pub fn record_roots(paths: &InstallPaths) -> Result<()> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    roots::write(&home, paths)
}

/// The root `variable` names, canonical; `None` when it is unset or empty.
fn env_root(variable: &str) -> Result<Option<PathBuf>> {
    std::env::var_os(variable)
        .filter(|path| !path.is_empty())
        .map(|path| absolute_root(PathBuf::from(path), variable))
        .transpose()
}

/// `path`, which `source` supplied, canonical when it exists; an error when
/// it is relative. `install-assets` checks its explicit directory flags here.
pub(crate) fn absolute_root(path: PathBuf, source: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!(
            "{source} is the relative path {}: an install root must be an absolute path, \
             since a relative one resolves against each process's working directory",
            path.display()
        );
    }
    Ok(canonical_or_lexical(&path))
}

/// `path` with every symlink resolved when it exists; otherwise its longest
/// existing ancestor resolved and the rest appended as given, so a root not
/// created yet is spelled as it will resolve once it is.
pub(crate) fn canonical_or_lexical(path: &Path) -> PathBuf {
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
/// directory resolves (no home directory, a relative root).
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

/// Whether `hooks_dir` is the default Claude root's `<home>/.claude/hooks/loom`,
/// both compared canonical. Only there does the `~/.claude/hooks/loom` spelling
/// name the installed hooks.
pub(crate) fn is_default_claude_hooks_dir(hooks_dir: &Path, home: &Path) -> bool {
    let default = home.join(".claude").join(HOOKS_SUBDIR);
    canonical_or_lexical(&default) == canonical_or_lexical(hooks_dir)
}

#[cfg(test)]
#[path = "tests_resolve.rs"]
mod tests;
