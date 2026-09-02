//! User-level configuration at `~/.loom/config.toml`.
//!
//! Distinct from, and a fallback tier under, a project's workspace config at
//! `<repo>/.loom/work/config.toml` (see [`crate::fs::work_dir::Config`], the
//! `.loom/work/` type). This file holds settings that make sense per operator
//! rather than per plan: whether loom checks for updates, which terminal
//! backend `loom run` defaults to, and the default context ceiling. `loom
//! config` (`crate::commands::config`) is the only writer; every other
//! consumer reads through [`UserConfig::load`], which never fails.
//!
//! # Reads must not create `~/.loom/`
//!
//! [`UserConfig::load`] and [`UserConfig::load_strict`] use a plain
//! `std::fs::read_to_string`, treating `NotFound` as "absent". They
//! deliberately avoid [`crate::fs::locking::locked_read`] and friends: those
//! helpers lock the file's *parent directory*, and acquiring that lock
//! creates the parent if it does not exist (see
//! `crate::fs::locking::lock_parent_dir`) — which would make a pure read like
//! `loom config --list` materialize `~/.loom/` on disk.
//!
//! # Writes are locked and atomic
//!
//! [`set`] goes through [`crate::fs::locking::locked_update`], which holds the
//! exclusive parent-directory lock across the whole read-modify-write,
//! presents a missing file as an empty string, creates `~/.loom/` as a side
//! effect of locking, and finishes with a crash-atomic temp+rename write. This
//! is the ONLY path that may create `~/.loom/`. loom is invoked concurrently
//! from shell hooks, so this race is real.
//!
//! # The lib test binary never reads or writes the real home
//!
//! [`UserConfig::load`]/[`UserConfig::load_strict`] resolve their path
//! through `read_config_path`, and [`set`] through `write_config_path`.
//! Those two are private, so these are plain code spans: a bracketed link
//! would resolve only under `--document-private-items`, which the docs gate
//! does not pass.
//! Both are `config_path()` in production but, in `#[cfg(test)]`, the same
//! per-thread redirect (`redirect_user_config`, itself test-only) that defaults
//! to "no user config" when unset. Sharing one redirect between the read and
//! write seams lets a test do a real set/read round trip against a temp path.
//! Any lib test that reaches a loader without installing a redirect therefore
//! sees an all-default [`UserConfig`], never `~/.loom/config.toml`. This
//! matters because `loom config` (`crate::commands::config`) is the tool that
//! CREATES that file: without this seam, the whole suite would depend on the
//! developer running these tests never having run `loom config`, and would
//! start failing the moment they had.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS;
use crate::models::session::SessionBackendKind;

pub mod keys;

use keys::KeySpec;

#[cfg(test)]
mod redirect;

#[cfg(test)]
pub(crate) use redirect::{redirect_user_config, UserConfigRedirect};

#[cfg(test)]
mod tests;

/// Where a resolved key's value came from.
///
/// `loom config` reads only the user config, never a workspace config, so
/// these two are the whole space — a workspace `[terminal]`/`[context]`
/// section is a separate resolution tier that
/// [`crate::fs::work_dir::read_terminal_config`]/
/// [`crate::fs::work_dir::read_context_config`] merge in themselves, above
/// this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Explicitly set in `~/.loom/config.toml`.
    Set,
    /// Not set; the rendered value is the built-in default.
    Default,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Origin::Set => write!(f, "set"),
            Origin::Default => write!(f, "default"),
        }
    }
}

/// The user-level config resolved from `~/.loom/config.toml`.
///
/// Every field is `Option` so "explicitly set in the file" stays
/// distinguishable from "fell through to the built-in default": the resolved
/// getters below collapse that distinction for an ordinary caller, while
/// [`UserConfig::value_of`] and the `terminal_backend_set`/
/// `context_ceiling_tokens_set` accessors keep it for `loom config --list` and
/// for the workspace fallback tier in `crate::fs::work_dir`, both of which
/// need to know whether the user config actually set a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserConfig {
    update_check: Option<bool>,
    update_check_interval_hours: Option<u32>,
    terminal_backend: Option<SessionBackendKind>,
    context_ceiling_tokens: Option<u32>,
}

/// The absolute path to `~/.loom/config.toml`.
///
/// Errors only when the home directory cannot be resolved. Never creates
/// anything — this is a pure path computation.
pub fn config_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".loom").join("config.toml"))
        .ok_or_else(|| {
            anyhow::anyhow!("could not resolve the home directory for ~/.loom/config.toml")
        })
}

/// The file the read paths (`UserConfig::load`/`UserConfig::load_strict`)
/// consult, or `None` when no user config file is in play. Fallible because
/// an unresolvable home directory is a real failure that `load_strict` must
/// surface; `load` discards it.
///
/// In production this is `config_path()`. In the lib test binary it is the
/// per-thread redirect from [`redirect::redirect_user_config`] and NOTHING
/// else: an unset redirect means "no user config", so a test can never
/// observe — or be broken by — the operator's own `~/.loom/config.toml`.
/// `loom config` itself creates that file, so the suite must not depend on
/// nobody having one.
#[cfg(not(test))]
fn read_config_path() -> Result<Option<PathBuf>> {
    config_path().map(Some)
}

#[cfg(test)]
fn read_config_path() -> Result<Option<PathBuf>> {
    Ok(redirect::test_redirect())
}

/// The file [`set`] writes to. Unlike the read side there is no "absent"
/// case: a set with nowhere to write is an error. Shares the same
/// thread-local redirect as [`read_config_path`], so a test installing one
/// redirect can do a real set/read round trip against a single temp path.
#[cfg(not(test))]
fn write_config_path() -> Result<PathBuf> {
    config_path()
}

#[cfg(test)]
fn write_config_path() -> Result<PathBuf> {
    redirect::test_redirect()
        .ok_or_else(|| anyhow::anyhow!("no user config redirect installed for this test"))
}

impl UserConfig {
    /// Load `~/.loom/config.toml` for production consumers. Never fails: an
    /// absent, unreadable, or unparseable file yields an all-`None`
    /// [`UserConfig`] (every getter then returns the built-in default). A
    /// broken user config must never take down `loom run`. Does not create
    /// `~/.loom`.
    pub fn load() -> UserConfig {
        let Ok(Some(path)) = read_config_path() else {
            return UserConfig::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => parse_document(&text).unwrap_or_default(),
            Err(_) => UserConfig::default(),
        }
    }

    /// Load `~/.loom/config.toml`, surfacing a read or parse failure. Used
    /// only by `loom config` (`crate::commands::config`), which wants to
    /// report a broken user config rather than silently fall back to
    /// defaults. Does not create `~/.loom`.
    pub fn load_strict() -> Result<UserConfig> {
        let Some(path) = read_config_path()? else {
            return Ok(UserConfig::default());
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(UserConfig::default()),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        parse_document(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Whether loom checks for updates on startup. Default: `true`.
    pub fn update_check(&self) -> bool {
        self.update_check.unwrap_or(true)
    }

    /// Hours between update checks. Default: `24`.
    pub fn update_check_interval_hours(&self) -> u32 {
        self.update_check_interval_hours.unwrap_or(24)
    }

    /// Terminal backend `loom run` defaults to. Default: native.
    pub fn terminal_backend(&self) -> SessionBackendKind {
        self.terminal_backend.unwrap_or_default()
    }

    /// `terminal.backend`, only when the user config explicitly set it — the
    /// accessor `crate::fs::work_dir::read_terminal_config`'s fallback tier
    /// needs, since it must not treat "fell through to the built-in" as a
    /// value worth applying over an absent workspace section any differently
    /// than the built-in itself.
    pub fn terminal_backend_set(&self) -> Option<SessionBackendKind> {
        self.terminal_backend
    }

    /// Default context ceiling, in resident tokens, for a stage's agent
    /// session. Default: [`DEFAULT_CONTEXT_CEILING_TOKENS`].
    pub fn context_ceiling_tokens(&self) -> u32 {
        self.context_ceiling_tokens
            .unwrap_or(DEFAULT_CONTEXT_CEILING_TOKENS)
    }

    /// `context.ceiling_tokens`, only when the user config explicitly set it
    /// — see [`UserConfig::terminal_backend_set`] for why the workspace
    /// fallback tier needs this rather than the resolved getter.
    pub fn context_ceiling_tokens_set(&self) -> Option<u32> {
        self.context_ceiling_tokens
    }

    /// The rendered value and origin for `spec`, for `loom config --list` and
    /// `loom config -k <key>`.
    pub fn value_of(&self, spec: &KeySpec) -> (String, Origin) {
        match spec.name {
            "update.check" => (
                self.update_check().to_string(),
                self.origin_of(self.update_check),
            ),
            "update.check_interval_hours" => (
                self.update_check_interval_hours().to_string(),
                self.origin_of(self.update_check_interval_hours),
            ),
            "terminal.backend" => (
                self.terminal_backend().to_string(),
                self.origin_of(self.terminal_backend),
            ),
            "context.ceiling_tokens" => (
                self.context_ceiling_tokens().to_string(),
                self.origin_of(self.context_ceiling_tokens),
            ),
            other => unreachable!("value_of: {other} is not in keys::KEYS"),
        }
    }

    fn origin_of<T>(&self, set: Option<T>) -> Origin {
        if set.is_some() {
            Origin::Set
        } else {
            Origin::Default
        }
    }

    /// The fully resolved config (every key, its effective value) as TOML,
    /// sections in `[context]`, `[terminal]`, `[update]` order — the shape
    /// `loom config --print` renders.
    pub fn to_toml_string(&self) -> String {
        format!(
            "[context]\nceiling_tokens = {}\n\n[terminal]\nbackend = \"{}\"\n\n[update]\ncheck = {}\ncheck_interval_hours = {}\n",
            self.context_ceiling_tokens(),
            self.terminal_backend(),
            self.update_check(),
            self.update_check_interval_hours(),
        )
    }
}

/// Parse `text` as `~/.loom/config.toml`, validating every present key
/// against its type. `pub(crate)` so `UserConfig::load`/`load_strict` share
/// one parser, and so tests can exercise parsing/rendering against a TOML
/// string without touching the real `$HOME`.
pub(crate) fn parse_document(text: &str) -> Result<UserConfig> {
    let doc: DocumentMut = text.parse().context("invalid TOML")?;
    Ok(UserConfig {
        update_check: get_bool(&doc, "update", "check")?,
        update_check_interval_hours: get_u32(&doc, "update", "check_interval_hours")?,
        terminal_backend: get_backend(&doc)?,
        context_ceiling_tokens: get_u32(&doc, "context", "ceiling_tokens")?,
    })
}

fn section_item<'a>(
    doc: &'a DocumentMut,
    section: &str,
    field: &str,
) -> Option<&'a toml_edit::Item> {
    doc.get(section)?.get(field)
}

fn get_bool(doc: &DocumentMut, section: &str, field: &str) -> Result<Option<bool>> {
    match section_item(doc, section, field) {
        None => Ok(None),
        Some(item) => item.as_bool().map(Some).ok_or_else(|| {
            anyhow::anyhow!(
                "{section}.{field}: expected a bool, found {}",
                item.type_name()
            )
        }),
    }
}

fn get_u32(doc: &DocumentMut, section: &str, field: &str) -> Result<Option<u32>> {
    match section_item(doc, section, field) {
        None => Ok(None),
        Some(item) => {
            let int = item.as_integer().ok_or_else(|| {
                anyhow::anyhow!(
                    "{section}.{field}: expected an integer, found {}",
                    item.type_name()
                )
            })?;
            u32::try_from(int)
                .map(Some)
                .map_err(|_| anyhow::anyhow!("{section}.{field}: {int} is out of range for a u32"))
        }
    }
}

fn get_backend(doc: &DocumentMut) -> Result<Option<SessionBackendKind>> {
    match section_item(doc, "terminal", "backend") {
        None => Ok(None),
        Some(item) => {
            let raw = item.as_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "terminal.backend: expected a string, found {}",
                    item.type_name()
                )
            })?;
            match raw {
                "native" => Ok(Some(SessionBackendKind::Native)),
                "tmux" => Ok(Some(SessionBackendKind::Tmux)),
                other => Err(anyhow::anyhow!(
                    "terminal.backend: {other:?} is not one of native, tmux"
                )),
            }
        }
    }
}

/// Read-modify-write `spec`'s section/field in `~/.loom/config.toml`,
/// creating the section if absent. Comments and unrelated keys are preserved
/// verbatim (`toml_edit::DocumentMut`). This is the only path that may create
/// `~/.loom/` — see the module docs on why the read path must not.
pub fn set(spec: &KeySpec, value: toml_edit::Value) -> Result<()> {
    set_in(&write_config_path()?, spec, value)
}

/// [`set`] factored over an explicit path so tests can exercise the
/// read-modify-write behavior against a temp file instead of the real
/// `~/.loom/config.toml`.
pub(crate) fn set_in(path: &Path, spec: &KeySpec, value: toml_edit::Value) -> Result<()> {
    crate::fs::locking::locked_update(path, |existing| {
        let mut doc: DocumentMut = existing.parse().with_context(|| {
            format!(
                "refusing to rewrite unparseable user config at {}",
                path.display()
            )
        })?;
        let table = doc
            .entry(spec.section)
            .or_insert(toml_edit::table())
            .as_table_like_mut()
            .ok_or_else(|| {
                anyhow::anyhow!("[{}] in {} is not a table", spec.section, path.display())
            })?;
        table.insert(spec.field, toml_edit::Item::Value(value));
        Ok(doc.to_string())
    })
}
