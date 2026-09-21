//! The two config files the editor edits, and how each one is written and
//! named on screen.
//!
//! `loom config` used to know about one file. The project tier has existed for
//! as long as `.loom/work/config.toml` has, and the dashboard already edited
//! it; the screen that is supposed to be the authority on loom's settings must
//! not be the one surface that pretends the tier is not there.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::user_config::keys::KeySpec;
use crate::user_config::workspace::Workspace;
use crate::user_config::ConfigValue;

/// Which config file the editor is pointed at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scope {
    /// `~/.loom/config.toml`, the operator's own settings.
    User,
    /// `<repo>/.loom/work/config.toml`, this tree's settings.
    Project,
}

/// Both scopes in tab order, for the loops that must visit each exactly once.
pub(crate) const SCOPES: [Scope; 2] = [Scope::User, Scope::Project];

impl Scope {
    /// This scope's slot in a row's per-scope pending array. Staged edits are
    /// indexed rather than paired so switching tabs cannot lose one.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::User => 0,
            Self::Project => 1,
        }
    }

    /// The scope `Tab` moves to from here.
    pub(crate) const fn other(self) -> Self {
        match self {
            Self::User => Self::Project,
            Self::Project => Self::User,
        }
    }

    /// The lower-case word the status line and the inspector use.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }

    /// The tab's label: the tier, then what it governs, because "user" and
    /// "project" alone do not tell a first-time operator which one wins.
    pub(crate) const fn tab_label(self) -> &'static str {
        match self {
            Self::User => "USER · global",
            Self::Project => "PROJECT · this repo",
        }
    }
}

/// Write or clear `spec` in the file `scope` names.
///
/// Both tiers go through their own module's locked read-modify-write, never a
/// read/modify/write here: `loom config` runs beside the daemon and the shell
/// hooks, and a lost update in a config file is silent.
pub(crate) fn write(
    scope: Scope,
    workspace: Option<&Workspace>,
    spec: &KeySpec,
    value: Option<ConfigValue>,
) -> Result<()> {
    match scope {
        Scope::User => match value {
            Some(value) => crate::user_config::set(spec, value),
            None => crate::user_config::unset(spec),
        },
        Scope::Project => workspace
            .ok_or_else(|| anyhow!("no .loom/work in this tree to write the project scope to"))?
            .write(spec, value),
    }
    .map(|_| ())
}

/// The user config's path as the operator recognizes it, `$HOME` collapsed
/// to `~`.
///
/// The path is unresolvable only when there is no home directory at all;
/// naming the canonical location still tells the operator more than an empty
/// tab would, and the save itself reports the real failure.
pub(crate) fn user_path_label() -> String {
    crate::user_config::config_path().map_or_else(
        |_| "~/.loom/config.toml".to_owned(),
        |path| collapse_home(&path),
    )
}

/// The project config's path, relative to `base` when it sits under it.
///
/// `None` is "this tree has no project tier", which the scope tab renders as
/// its own state rather than as an empty path.
pub(crate) fn project_path_label(root: Option<&Path>, base: &Path) -> Option<String> {
    let path: PathBuf = root?.join("config.toml");
    Some(match path.strip_prefix(base) {
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    })
}

fn collapse_home(path: &Path) -> String {
    let Some(home) = dirs::home_dir() else {
        return path.display().to_string();
    };
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
