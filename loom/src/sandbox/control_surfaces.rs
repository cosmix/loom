//! What a loom session could write, and the control surfaces no propagated
//! permission may name (`doc/plans/PLAN-loom-state-confinement.md`, sections
//! 11 and 12).
//!
//! Pure: every input is a parameter. Nothing here reads the process
//! environment or touches the filesystem, so the per-spawn checks and the
//! phase-3 `loom run` refusals can share one answer.
//!
//! Accepted gap: this filter reads rule TEXT only, so a rule naming a path
//! that is itself a symlink into a control surface is not caught here. The
//! phase-3 OS-level deny rules on the control surfaces win over any allow
//! regardless of the path spelling that reached them, and the sandbox
//! resolves symlinks before matching, so that layer still refuses the write.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use super::PACKAGE_MANAGER_CACHE_WRITE_PATHS;
use crate::codex::CODEX_SANDBOX_WRITE_PATHS;

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

/// The codex lane's hook and configuration files, relative to the home
/// directory.
const CODEX_CONTROL_PATHS: [&str; 3] = [".codex/hooks", ".codex/hooks.json", ".codex/config.toml"];

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

fn push_unique(roots: &mut Vec<PathBuf>, root: PathBuf) {
    if !roots.contains(&root) {
        roots.push(root);
    }
}

/// The absolute paths a propagated permission rule must never name, on top of
/// the [`CONTROL_COMPONENTS`] every rule is checked for.
#[derive(Debug, Clone)]
pub(crate) struct ControlSurfaces {
    roots: Vec<PathBuf>,
    home: Option<PathBuf>,
}

impl ControlSurfaces {
    /// The resolved state root, the scratch root, every hooks directory, and
    /// under `home` the codex lane's hooks and config plus `~/.loom`.
    pub(crate) fn new(
        state_root: &Path,
        scratch_root: Option<&Path>,
        hooks_dirs: &[PathBuf],
        home: Option<&Path>,
    ) -> Self {
        let mut roots = vec![state_root.to_path_buf()];
        roots.extend(scratch_root.map(Path::to_path_buf));
        roots.extend(hooks_dirs.iter().cloned());
        if let Some(home) = home {
            roots.extend(CODEX_CONTROL_PATHS.iter().map(|path| home.join(path)));
            roots.push(home.join(".loom"));
        }
        Self {
            roots,
            home: home.map(Path::to_path_buf),
        }
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
mod tests {
    use super::*;

    fn inputs(allow_write: &[String], codex_licensed: bool) -> WritableRootInputs<'_> {
        WritableRootInputs {
            repo_root: Path::new("/repo"),
            allow_write,
            codex_licensed,
            scratch_root: Path::new("/home/op/.cache/loom/scratch"),
            home: Some(Path::new("/home/op")),
            tmpdir: Some(Path::new("/var/tmp/op")),
        }
    }

    #[test]
    fn writable_roots_cover_every_input() {
        let allow_write = vec![
            "/srv/out/**".to_string(),
            "~/data".to_string(),
            "//abs/grant".to_string(),
            "loom/target".to_string(),
        ];
        let roots = session_writable_roots(&inputs(&allow_write, true));
        for expected in [
            "/repo",
            "/srv/out",
            "/home/op/data",
            "/abs/grant",
            "/repo/loom/target",
            "/home/op/.cargo/registry",
            "/home/op/.bun/install/cache",
            "/home/op/.codex",
            "/home/op/.claude/plugins/data/codex-openai-codex",
            "/home/op/.cache/loom/scratch",
            "/tmp",
            "/var/tmp/op",
        ] {
            assert!(
                roots.contains(&PathBuf::from(expected)),
                "missing {expected}: {roots:?}"
            );
        }
    }

    #[test]
    fn writable_roots_omit_the_codex_paths_unless_the_lane_is_licensed() {
        let roots = session_writable_roots(&inputs(&[], false));
        assert!(!roots.contains(&PathBuf::from("/home/op/.codex")));
        assert!(roots.contains(&PathBuf::from("/home/op/.cargo/registry")));
    }

    #[test]
    fn grant_root_reads_every_spelling() {
        let base = Path::new("/repo");
        let home = Some(Path::new("/home/op"));
        assert_eq!(
            grant_root("doc/loom/knowledge/**", base, home),
            Some(PathBuf::from("/repo/doc/loom/knowledge"))
        );
        assert_eq!(
            grant_root("**/*.rs", base, home),
            Some(PathBuf::from("/repo"))
        );
        assert_eq!(grant_root("~/cache/x", base, None), None);
        assert_eq!(grant_root("../../escape", base, home), None);
        assert_eq!(grant_root("  ", base, home), None);
    }

    fn surfaces() -> ControlSurfaces {
        ControlSurfaces::new(
            Path::new("/repo/.loom/work"),
            Some(Path::new("/run/user/1000/loom/scratch")),
            &[PathBuf::from("/opt/loom-hooks")],
            Some(Path::new("/home/op")),
        )
    }

    #[test]
    fn names_every_control_surface() {
        let surfaces = surfaces();
        for rule in [
            "Edit(.loom/work/handoffs/**)",
            "Edit(.work/signals/x.md)",
            "Read(//repo/.loom/work/config.toml)",
            "Edit(.worktrees/s1/**)",
            "Edit(.claude/settings.json)",
            "Write(~/.claude/hooks/loom/x.sh)",
            "Edit(//opt/loom-hooks/loom-relay.sh)",
            "Edit(~/.loom/config.toml)",
            "Edit(//run/user/1000/loom/scratch/session-1/**)",
            "Edit(~/.codex/hooks.json)",
            "Edit(~/.codex/**)",
            "Edit(**)",
            "Edit(../**)",
            "Edit",
            "Bash(cp x .claude/settings.json)",
            "Bash(rm -rf /opt/loom-hooks/old)",
        ] {
            assert!(surfaces.names(rule), "{rule} must be dropped");
        }
    }

    #[test]
    fn names_every_control_component_regardless_of_case() {
        let surfaces = surfaces();
        for rule in [
            "Edit(.LOOM/work/handoffs/**)",
            "Edit(.Work/signals/x.md)",
            "Edit(.WorkTrees/s1/**)",
            "Edit(.Claude/settings.json)",
        ] {
            assert!(surfaces.names(rule), "{rule} must be dropped");
        }
    }

    #[test]
    fn names_a_root_prefix_regardless_of_case() {
        // Neither "opt", "loom-hooks", "run", "user" nor "scratch" is a
        // `CONTROL_COMPONENTS` entry, so these can only be caught by the
        // root-prefix comparison, not the component check above.
        let surfaces = surfaces();
        for rule in [
            "Edit(//OPT/loom-hooks/loom-relay.sh)",
            "Edit(//opt/LOOM-HOOKS/loom-relay.sh)",
            "Edit(//RUN/user/1000/loom/scratch/session-1/**)",
        ] {
            assert!(surfaces.names(rule), "{rule} must be dropped");
        }
    }

    #[test]
    fn leaves_ordinary_rules_alone() {
        let surfaces = surfaces();
        for rule in [
            "Bash(cargo test:*)",
            "Edit(loom/src/**)",
            "WebFetch(domain:docs.rs)",
            "Read(//usr/share/doc/**)",
            "Edit(~/.cargo/registry/**)",
            "mcp__github__search",
            "Bash",
        ] {
            assert!(!surfaces.names(rule), "{rule} must be kept");
        }
    }
}
