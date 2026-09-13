//! What a loom session could write, the control surfaces no propagated
//! permission may name, and the write denies every session capsule carries
//! (`doc/plans/PLAN-loom-state-confinement.md`, sections 10 to 12).
//!
//! Pure: every input is a parameter. Nothing here reads the process
//! environment, and only `codex_plugin_entries` touches the filesystem (it
//! lists `~/.claude/plugins` at spawn), so the per-spawn checks and the
//! `loom run` refusals can share one answer.
//!
//! Accepted gap: the propagation filter reads rule TEXT only, so a rule
//! naming a path that is itself a symlink into a control surface is not
//! caught by it. The capsule's deny rules on the control surfaces
//! (`session_denies`) win over any allow regardless of the path spelling
//! that reached them, and the sandbox resolves symlinks before matching, so
//! that layer still refuses the write.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use super::PACKAGE_MANAGER_CACHE_WRITE_PATHS;
use crate::codex::CODEX_SANDBOX_WRITE_PATHS;

mod session_denies;

pub(crate) use session_denies::{codex_plugin_entries, session_denies, DenyInputs, SessionDenies};

/// Characters that end the literal part of a path pattern.
const GLOB_CHARS: [char; 4] = ['*', '?', '[', '{'];

/// Path components that mark loom state or Claude Code configuration wherever
/// they appear: the state root in both layouts, stage worktrees, and every
/// `.claude` directory (hooks, settings, projects, plugins).
const CONTROL_COMPONENTS: [&str; 4] = [".loom", ".work", ".worktrees", ".claude"];

/// Tools whose rule argument is a single path pattern.
const PATH_TOOLS: [&str; 7] = [
    "Edit",
    "Write",
    "MultiEdit",
    "NotebookEdit",
    "Read",
    "Glob",
    "Grep",
];

/// Characters that separate the words of any other rule's argument, such as
/// `Bash(git -C <dir> status)` or `Bash(npm run test:*)`.
const WORD_SEPARATORS: [char; 12] = ['\'', '"', '=', ':', ';', ',', '(', ')', '<', '>', '|', '&'];

/// The control surfaces under the operator's home directory (plan section
/// 11), relative to it, in rule form: a directory ends in `/**`, a file does
/// not. Every capsule denies writing each one, the codex lane's hook and
/// configuration files included even inside its `~/.codex` grant, and no
/// propagated rule may name one. `~/.claude/plugins` is not listed: the
/// codex lane writes inside it, so `session_denies` handles it apart.
const HOME_SURFACES: [&str; 12] = [
    ".claude/hooks/**",
    ".claude/settings.json",
    ".claude.json",
    ".loom/**",
    ".claude/projects/**",
    ".claude/agents/**",
    ".claude/skills/**",
    ".claude/commands/**",
    ".claude/loom-skill-catalog/**",
    ".codex/hooks/**",
    ".codex/hooks.json",
    ".codex/config.toml",
];

/// Inputs to [`session_writable_roots`].
pub(crate) struct WritableRootInputs<'a> {
    /// The repository root; every stage worktree lives under it.
    pub repo_root: &'a Path,
    /// Every stage's merged `allowWrite` entries, as `merge_config` and
    /// `expand_paths` leave them (relative, `~/`, `/` or `//` spellings).
    pub allow_write: &'a [String],
    /// Whether any stage licenses the codex lane.
    pub codex_licensed: bool,
    /// The root every session scratch directory lives under.
    pub scratch_root: &'a Path,
    /// The operator's home directory, for `~/` entries.
    pub home: Option<&'a Path>,
    /// The daemon's `$TMPDIR`.
    pub tmpdir: Option<&'a Path>,
}

/// Every root a loom session could write: the repository, each stage's
/// granted `allowWrite` directory, the package-manager caches, the codex
/// state directories when that lane is licensed, the scratch root, `/tmp` and
/// `$TMPDIR`. A glob entry contributes its literal directory; an entry that
/// climbs with `../` is never granted, so it contributes nothing.
pub(crate) fn session_writable_roots(inputs: &WritableRootInputs<'_>) -> Vec<PathBuf> {
    let mut grants: Vec<&str> = inputs.allow_write.iter().map(String::as_str).collect();
    grants.extend(PACKAGE_MANAGER_CACHE_WRITE_PATHS);
    if inputs.codex_licensed {
        grants.extend(CODEX_SANDBOX_WRITE_PATHS);
    }
    let mut roots = vec![inputs.repo_root.to_path_buf()];
    for grant in grants {
        if let Some(root) = grant_root(grant, inputs.repo_root, inputs.home) {
            push_unique(&mut roots, root);
        }
    }
    push_unique(&mut roots, inputs.scratch_root.to_path_buf());
    push_unique(&mut roots, PathBuf::from("/tmp"));
    if let Some(tmpdir) = inputs.tmpdir {
        push_unique(&mut roots, tmpdir.to_path_buf());
    }
    roots
}

/// The literal directory an `allowWrite` entry grants: `~/` resolved against
/// `home`, `//` collapsed to `/`, a relative entry resolved against `base`,
/// and everything from the first glob component on dropped. `None` for an
/// empty entry, one that climbs with `../`, or a `~/` entry with no home.
fn grant_root(entry: &str, base: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() || entry.contains("../") {
        return None;
    }
    let path = expand(entry, home);
    if path.starts_with("~") {
        return None;
    }
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    Some(literal_prefix(&path))
}

/// `~/x` resolved against `home` (left literal without one) and `//x`
/// collapsed to `/x`; any other spelling unchanged.
fn expand(entry: &str, home: Option<&Path>) -> PathBuf {
    let home_relative = entry.strip_prefix("~/").or((entry == "~").then_some(""));
    match (home_relative, home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => match entry.strip_prefix("//") {
            Some(rest) => Path::new("/").join(rest),
            None => PathBuf::from(entry),
        },
    }
}

/// `path` up to its first component holding a glob character, with `.`
/// components dropped.
fn literal_prefix(path: &Path) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) if is_glob(name) => break,
            other => prefix.push(other),
        }
    }
    prefix
}

fn is_glob(name: &OsStr) -> bool {
    name.to_string_lossy()
        .chars()
        .any(|c| GLOB_CHARS.contains(&c))
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

/// A `HOME_SURFACES` entry split into its literal path and whether it names
/// a directory.
fn surface_path(surface: &str) -> (&str, bool) {
    match surface.strip_suffix("/**") {
        Some(dir) => (dir, true),
        None => (surface, false),
    }
}

/// The absolute paths a propagated permission rule must never name, on top of
/// the [`CONTROL_COMPONENTS`] every rule is checked for.
#[derive(Debug, Clone)]
pub(crate) struct ControlSurfaces {
    roots: Vec<PathBuf>,
    executable_dirs: Vec<PathBuf>,
    home: Option<PathBuf>,
}

impl ControlSurfaces {
    /// The resolved state root, the scratch root, every directory holding an
    /// executable loom's hooks run (the hooks directory, plus the
    /// operator-owned `LOOM_HOOK_PATH` entries and `dirname(LOOM_BIN)` where
    /// the caller knows them), and under `home` every `HOME_SURFACES` entry.
    pub(crate) fn new(
        state_root: &Path,
        scratch_root: Option<&Path>,
        executable_dirs: &[PathBuf],
        home: Option<&Path>,
    ) -> Self {
        let mut roots = vec![state_root.to_path_buf()];
        roots.extend(scratch_root.map(Path::to_path_buf));
        roots.extend(executable_dirs.iter().cloned());
        if let Some(home) = home {
            roots.extend(
                HOME_SURFACES
                    .iter()
                    .map(|surface| home.join(surface_path(surface).0)),
            );
        }
        Self {
            roots,
            executable_dirs: executable_dirs.to_vec(),
            home: home.map(Path::to_path_buf),
        }
    }

    /// The directories holding executables loom's hooks run; every capsule
    /// denies writing them.
    pub(crate) fn executable_dirs(&self) -> &[PathBuf] {
        &self.executable_dirs
    }

    /// The operator's home directory, when known.
    pub(crate) fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// Whether `rule`, a `permissions.allow` entry, names a control surface.
    ///
    /// A path tool's rule names one when its pattern holds a
    /// [`CONTROL_COMPONENTS`] entry, lies under one of the roots, reaches one
    /// through a glob over an ancestor, or covers the session's own directory
    /// (a bare tool name, an empty literal part, or `..`). Any other rule
    /// names one when a word of its argument is such a path.
    pub(crate) fn names(&self, rule: &str) -> bool {
        let Some((tool, argument)) = split_rule(rule) else {
            return PATH_TOOLS.contains(&rule.trim());
        };
        if PATH_TOOLS.contains(&tool) {
            return self.pattern_names(argument, true);
        }
        argument
            .split(|c: char| c.is_whitespace() || WORD_SEPARATORS.contains(&c))
            .any(|word| !word.is_empty() && self.pattern_names(word, false))
    }

    fn pattern_names(&self, pattern: &str, path_rule: bool) -> bool {
        let path = expand(pattern.trim(), self.home.as_deref());
        let marked = path.components().any(|component| {
            matches!(component, Component::Normal(name) if CONTROL_COMPONENTS
                .iter()
                .any(|c| name.to_string_lossy().eq_ignore_ascii_case(c)))
        });
        if marked {
            return true;
        }
        let prefix = literal_prefix(&path);
        if !prefix.is_absolute() {
            return path_rule
                && (prefix.as_os_str().is_empty()
                    || prefix.components().any(|c| c == Component::ParentDir));
        }
        let covers_descendants = path_rule && prefix != path;
        self.roots.iter().any(|root| {
            starts_with_ci(&prefix, root) || (covers_descendants && starts_with_ci(root, &prefix))
        })
    }
}

/// Case-insensitive [`Path::starts_with`]: the exact comparison misses a rule
/// spelled in a different case from a stored root on a case-insensitive
/// filesystem (macOS APFS/HFS+), where both resolve to the same file.
/// ASCII-folded only — see the module doc's accepted gap.
fn starts_with_ci(path: &Path, prefix: &Path) -> bool {
    fold(path).starts_with(fold(prefix))
}

fn fold(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_ascii_lowercase())
}

/// `Tool(argument)` split into its two parts; `None` for a bare tool name.
fn split_rule(rule: &str) -> Option<(&str, &str)> {
    let open = rule.find('(')?;
    let argument = rule[open + 1..].strip_suffix(')')?;
    Some((rule[..open].trim(), argument))
}

#[cfg(test)]
mod tests;
