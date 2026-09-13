//! The workspace tier of a config key, read once per request.
//!
//! # One shadowing rule: every workspace-backed key resolves per KEY
//!
//! `[terminal]`, `[context]`, `[pressure]` and `[models]` all shadow
//! `~/.loom/config.toml` a KEY at a time: a present section that omits a key
//! falls through to the user tier for that key rather than shadowing it with
//! a built-in the operator never asked for — a plan's stage can leave
//! `standard_effort` unset while pinning `standard_model`, and the effort
//! must still come from whatever the user tier (or the built-in) says.
//! [`resolve`] is the one `match` that names which keys the workspace tier
//! backs and what each resolves to; [`Workspace::shadows`] is what a caller
//! asks instead of re-deriving that from the key's section.
//!
//! `context.ceiling_tokens` carries one qualification: the project supplies
//! it by setting either `ceiling_tokens` or `model_window_tokens` — a window
//! is a project-tier statement about the
//! ceiling too, so a user ceiling sized for a different window must not
//! override a plan's smaller one. `ContextConfig::resolve_with_user_ceiling`
//! (`pub(crate)`, so a plain code span rather than a link) is the one place
//! that predicate is decided; `resolve` calls through to it rather than
//! re-deriving it here.
//!
//! [`Workspace::value_of`] answers "what does the project tier's own value for
//! this key look like" regardless of whether it shadows — for a key the file
//! does not set, that is the user tier's value, the one the fallback leaves in
//! force, while [`Workspace::shadows`] is what keeps the project tier out of
//! `effective` in that case.
//!
//! `config_api::tests` pins all of this against the runtime readers —
//! [`crate::fs::work_dir::read_terminal_config`] and
//! [`crate::fs::work_dir::resolve_context_ceiling_tokens`] for the two keys
//! above, [`crate::fs::work_dir::read_pressure_config`] and
//! [`crate::fs::work_dir::resolve_stage_model_effort`] for the fourteen
//! `[pressure]`/`[models]` ones — because a settings page that disagrees with
//! the daemon about the value in force is worse than no settings page.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use toml_edit::DocumentMut;

use crate::fs::work_dir::{read_config, ContextConfig, WorkDir};
use crate::models::session::TerminalConfig;
use crate::user_config::keys::KeySpec;
use crate::user_config::UserConfig;

/// A served tree's `.loom/work` and its parsed `config.toml`.
pub(super) struct Workspace {
    /// The `.loom/work` directory itself — what the write path locks.
    root: PathBuf,
    /// The parsed config, an empty table when the file does not exist yet.
    doc: toml::Value,
}

impl Workspace {
    /// The workspace under `base`, or `None` when the served tree has none.
    ///
    /// Absence is reported rather than created: a dashboard write must not
    /// materialize a workspace the operator never ran `loom init` for, so a
    /// project-scope write against `None` is a 409.
    pub(super) fn open(base: &Path) -> Result<Option<Self>> {
        let root = WorkDir::new(base)
            .context("failed to resolve the work directory")?
            .root()
            .to_path_buf();
        if !root.exists() {
            return Ok(None);
        }
        let text = read_config(&root)?.to_string();
        Ok(Some(Self {
            doc: parse(&text)?,
            root,
        }))
    }

    /// The `.loom/work` directory this workspace writes to.
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    /// The same view over an in-flight document, so the write path can report
    /// the values either side of its own edit using this exact resolution.
    pub(super) fn from_document(root: &Path, doc: &DocumentMut) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            doc: parse(&doc.to_string())?,
        })
    }

    /// Whether the file sets `spec`'s key itself.
    pub(super) fn has_key(&self, spec: &KeySpec) -> bool {
        self.doc
            .get(spec.section)
            .and_then(|section| section.get(spec.field))
            .is_some()
    }

    /// What the workspace tier resolves `spec` to.
    ///
    /// For a key the file does not supply, this is the user tier's value:
    /// that is what clearing the key (or never setting it) leaves in force.
    pub(super) fn value_of(&self, spec: &KeySpec) -> Result<String> {
        match resolve(spec, self.doc.get(spec.section).cloned()) {
            Some(value) => Ok(value?.unwrap_or_else(|| UserConfig::load().value_of(spec).0)),
            None => bail!("{} has no project scope", spec.name),
        }
    }

    /// Whether the project tier is the one in force for `spec`: the project
    /// supplies the key itself (directly, or via `resolve`'s
    /// `context.ceiling_tokens` qualification).
    pub(super) fn shadows(&self, spec: &KeySpec) -> bool {
        matches!(
            resolve(spec, self.doc.get(spec.section).cloned()),
            Some(Ok(Some(_)))
        )
    }
}

/// What the workspace tier resolves `spec` to in `section` — `None` when the
/// key has no workspace tier at all, `Some(Ok(None))` when it has one but the
/// project does not supply it (the caller falls through to the user tier),
/// `Some(Ok(Some(_)))` when the project supplies it.
///
/// The ONE place the workspace-backed keys are named. [`backs`] asks this
/// question rather than keeping a second list of key names beside it: a list
/// and a `match` that must agree drift, and the drift would land as a panic on
/// a live request rather than as a build failure.
///
/// The `_` arm matches on `spec.section` rather than `spec.name` so a key
/// added to `[pressure]`/`[models]` in the registry picks up this fallback
/// with no edit here — the two exact-name arms above stay name-matched so a
/// later key added to `[context]` cannot silently inherit `ContextConfig`'s
/// reading of a DIFFERENT field.
fn resolve(spec: &KeySpec, section: Option<toml::Value>) -> Option<Result<Option<String>>> {
    match spec.name {
        "terminal.backend" => Some(
            TerminalConfig::backend_from_section(section)
                .map(|backend| backend.map(|kind| kind.to_string())),
        ),
        "context.ceiling_tokens" => Some(
            ContextConfig::resolve_with_user_ceiling(section, None)
                .map(|(config, supplied)| supplied.then_some(config.ceiling_tokens.to_string())),
        ),
        _ => match spec.section {
            "pressure" | "models" => Some(Ok(section_key(section.as_ref(), spec))),
            _ => None,
        },
    }
}

/// Whether the workspace tier resolves `spec` at all — see [`resolve`], which
/// answers this by having an arm for the key or not.
pub(super) fn backs(spec: &KeySpec) -> bool {
    resolve(spec, None).is_some()
}

/// The section's own string for `spec.field`, or `None` when the section
/// omits it or is itself absent.
fn section_key(section: Option<&toml::Value>, spec: &KeySpec) -> Option<String> {
    section
        .and_then(|section| section.get(spec.field))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
}

fn parse(text: &str) -> Result<toml::Value> {
    if text.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(text).context("failed to parse the workspace config as TOML")
}
