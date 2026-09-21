//! Applying one `POST /api/config` update to the tier it names.
//!
//! Both tiers write through their own module's locked read-modify-write
//! (`user_config::write` / `Workspace::write`) rather than a bare
//! read/modify/write here: a dashboard write races the daemon, a shell hook and
//! any `loom config` the operator has open, and a lost update in a config file
//! is silent.

use anyhow::{anyhow, Result};

use crate::user_config;
use crate::user_config::keys::KeySpec;
use crate::user_config::workspace::Workspace;
use crate::user_config::ConfigValue;

use super::Scope;

/// Write or clear `spec` at `scope`, reporting what that scope resolved to
/// before and after.
///
/// `value` is `None` for an unset. `workspace` must be present for
/// [`Scope::Project`]; the caller has already turned its absence into a 409.
pub(super) fn apply(
    scope: Scope,
    spec: &KeySpec,
    value: Option<ConfigValue>,
    workspace: Option<&Workspace>,
) -> Result<(ConfigValue, ConfigValue)> {
    match scope {
        Scope::User => match value {
            Some(value) => user_config::set(spec, value),
            None => user_config::unset(spec),
        },
        Scope::Project => workspace
            .ok_or_else(|| anyhow!("no workspace to write the project scope to"))?
            .write(spec, value),
    }
}
