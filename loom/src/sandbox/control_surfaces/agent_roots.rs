//! The relocated Claude and Codex install roots, and the control surfaces
//! inside them.
//!
//! `HOME_SURFACES` names loom's assets at their `~/.claude` / `~/.codex`
//! defaults. An install moved by `LOOM_CLAUDECODE_INSTALL_DIR`,
//! `LOOM_CODEX_INSTALL_DIR` or a recorded root
//! (`assets::install::default_paths`) puts the hooks, skills, agents,
//! commands, skill catalog and Codex hook configuration where those denies
//! never reach, so a stage granted a write covering the root could plant
//! instructions every later session loads. [`AgentRoots`] carries each
//! relocated root so the capsule's denies and the propagation filter cover
//! its surfaces as well.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{codex_skill_surfaces, surface_path};
use crate::assets::install::canonical_or_lexical;

/// The control surfaces under a Claude root, relative to it, in the rule form
/// of `HOME_SURFACES`: what Claude Code runs or loads into every session.
const CLAUDE_ROOT_SURFACES: [&str; 7] = [
    "hooks/**",
    "settings.json",
    "CLAUDE.md",
    "agents/**",
    "skills/**",
    "commands/**",
    "loom-skill-catalog/**",
];

/// The control surfaces under a Codex root, relative to it: its hooks and
/// configuration, and the instructions loom installs for every codex session
/// to load (`AGENTS.md`, the skill catalog, and each skill directory from
/// [`codex_skill_surfaces`]). The codex lane's grant covers the whole root and
/// codex runs inside the session's sandbox, so its own runtime writes stay
/// open: `sessions/`, logs, databases, auth, `skills/.system` (its bundled
/// skills, refreshed at startup) and `rules/` (exec-policy approvals).
const CODEX_ROOT_SURFACES: [&str; 6] = [
    "hooks/**",
    "hooks.json",
    "config.toml",
    "AGENTS.md",
    "loom-skill-catalog/**",
    "prompts/**",
];

/// Each install root that lies somewhere other than its home default,
/// canonical; `None` for one at `~/.claude` / `~/.codex`, which
/// `HOME_SURFACES` covers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AgentRoots {
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
}

impl AgentRoots {
    /// `claude_dir` and `codex_dir`, absolute, each kept only when it differs
    /// from `<home>/.claude` / `<home>/.codex` once both are canonical; with no
    /// `home`, both are kept.
    pub(crate) fn relocated(claude_dir: &Path, codex_dir: &Path, home: Option<&Path>) -> Self {
        Self {
            claude: relocated(claude_dir, home.map(|home| home.join(".claude"))),
            codex: relocated(codex_dir, home.map(|home| home.join(".codex"))),
        }
    }

    /// The roots `assets::install::default_paths` resolves from the process
    /// environment. Its error passes through: a root that does not resolve
    /// (a relative override or record entry) would drop its surfaces' denies,
    /// so no caller may treat it as "not relocated".
    pub(crate) fn resolve(home: Option<&Path>) -> Result<Self> {
        let paths = crate::assets::install::default_paths()?;
        Ok(Self::relocated(&paths.claude_dir, &paths.codex_dir, home))
    }

    /// The relocated Claude root, if any.
    pub(crate) fn claude(&self) -> Option<&Path> {
        self.claude.as_deref()
    }

    /// The relocated Codex root, if any.
    pub(crate) fn codex(&self) -> Option<&Path> {
        self.codex.as_deref()
    }

    /// Every control surface under a relocated root, absolute, with whether
    /// it names a directory.
    pub(crate) fn surfaces(&self) -> Vec<(PathBuf, bool)> {
        let claude = CLAUDE_ROOT_SURFACES
            .iter()
            .map(|surface| surface.to_string());
        let codex = CODEX_ROOT_SURFACES
            .iter()
            .map(|surface| surface.to_string())
            .chain(codex_skill_surfaces());
        let roots: [(Option<&Path>, Vec<String>); 2] = [
            (self.claude(), claude.collect()),
            (self.codex(), codex.collect()),
        ];
        let mut surfaces = Vec::new();
        for (root, relative) in roots {
            let Some(root) = root else {
                continue;
            };
            for surface in &relative {
                let (path, dir) = surface_path(surface);
                surfaces.push((root.join(path), dir));
            }
        }
        surfaces
    }
}

/// `dir` canonical where it exists, so a deny names the path the sandbox
/// matches after resolving symlinks; `None` when it is `default`.
fn relocated(dir: &Path, default: Option<PathBuf>) -> Option<PathBuf> {
    let dir = canonical_or_lexical(dir);
    let default = default.map(|default| canonical_or_lexical(&default));
    (default.as_deref() != Some(dir.as_path())).then_some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_at_their_home_defaults_are_not_relocated() {
        let home = Path::new("/home/op");
        let roots = AgentRoots::relocated(&home.join(".claude"), &home.join(".codex/"), Some(home));
        assert_eq!(roots, AgentRoots::default());
        assert!(roots.surfaces().is_empty());
    }

    #[test]
    fn a_relocated_root_lists_its_surfaces_absolute() {
        let roots = AgentRoots::relocated(
            Path::new("/opt/claude"),
            Path::new("/home/op/.codex"),
            Some(Path::new("/home/op")),
        );
        assert_eq!(roots.codex(), None);
        let surfaces = roots.surfaces();
        assert!(surfaces.contains(&(PathBuf::from("/opt/claude/skills"), true)));
        assert!(surfaces.contains(&(PathBuf::from("/opt/claude/settings.json"), false)));
        assert_eq!(surfaces.len(), CLAUDE_ROOT_SURFACES.len());
    }
}
