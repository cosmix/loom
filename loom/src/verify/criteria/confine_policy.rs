//! The confined policy a command would run under, read without starting it.

use super::{prepare_confined, CommandSpec};
use crate::models::stage::CommandConfinement;
use crate::stage_cache::StageCache;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The launcher, its arguments and the variables it sets for a confined command.
pub(crate) struct ConfinedPolicy {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// The policy `spec` would run under in `tree` with the stage `cache`. It
/// creates the cache root and the command's scratch directories and starts no
/// process; it fails where the OS launcher is not installed.
pub(crate) fn confined_policy(
    spec: &CommandSpec,
    tree: &Path,
    cache: &StageCache,
) -> Result<ConfinedPolicy> {
    let confined = CommandConfinement::Confined;
    let prepared = prepare_confined(spec, Some(tree), Some(tree), confined, Some(cache))?;
    let wrapped = prepared
        .wrapped
        .as_ref()
        .context("a confined command has a wrapper")?;
    let text = |value: &std::ffi::OsStr| value.to_string_lossy().into_owned();
    Ok(ConfinedPolicy {
        program: PathBuf::from(wrapped.get_program()),
        args: wrapped.get_args().map(text).collect(),
        env: wrapped
            .get_envs()
            .filter_map(|(name, value)| Some((text(name), text(value?))))
            .collect(),
    })
}
