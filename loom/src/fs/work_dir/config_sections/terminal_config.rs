//! The `[terminal]` section of `.loom/work/config.toml`: the project tier of
//! the terminal backend choice. See [`read_terminal_config`] for the
//! fallback chain this section sits in.

use std::path::Path;

use anyhow::{Context, Result};

use crate::models::session::{SessionBackendKind, TerminalConfig};

/// Section name of the project-level terminal backend choice.
const TERMINAL_SECTION: &str = "terminal";

/// A raw shadow for `[terminal]`, so a present-but-keyless section can be told
/// from one that actually sets `backend` — the same job `ContextConfigRaw`
/// does for `[context]`.
#[derive(Debug, Default, serde::Deserialize)]
struct TerminalConfigRaw {
    backend: Option<SessionBackendKind>,
}

impl TerminalConfig {
    /// Resolve `backend` from an already-parsed `[terminal]` section value —
    /// the seam [`read_terminal_config`] and `/api/config`'s workspace
    /// resolution (`config_api::workspace::resolve`) share, so a malformed
    /// value fails the same way in both instead of the API silently treating
    /// it as unset.
    ///
    /// `Ok(None)` when the section is absent or omits `backend` (the caller
    /// falls through to the user tier); `Ok(Some(_))` when it sets one;
    /// `Err` when the section exists but fails to deserialize.
    pub(crate) fn backend_from_section(
        section: Option<toml::Value>,
    ) -> Result<Option<SessionBackendKind>> {
        let raw: TerminalConfigRaw = match section {
            Some(value) => value
                .try_into()
                .with_context(|| format!("Failed to deserialize [{TERMINAL_SECTION}] section"))?,
            None => TerminalConfigRaw::default(),
        };
        Ok(raw.backend)
    }
}

/// Read the persisted terminal backend config (`[terminal]`).
///
/// Resolves `backend` at the KEY level: the project's `[terminal]` supplies
/// it only when it sets that key itself; an absent key — section absent,
/// empty, or holding some other key — falls through to
/// `~/.loom/config.toml`'s `terminal.backend` (see
/// [`crate::user_config::UserConfig`]), then to `TerminalConfig::default()`
/// (native) when neither sets it. A malformed section still errors.
pub fn read_terminal_config(work_dir: &Path) -> Result<TerminalConfig> {
    let section = super::read_section::<toml::Value>(work_dir, TERMINAL_SECTION)?;
    let backend = TerminalConfig::backend_from_section(section)?
        .unwrap_or_else(|| crate::user_config::UserConfig::load().terminal_backend());
    Ok(TerminalConfig { backend })
}

/// Persist the terminal backend config (`[terminal]`).
pub fn write_terminal_config(work_dir: &Path, config: &TerminalConfig) -> Result<()> {
    super::write_section(work_dir, TERMINAL_SECTION, config)
}
