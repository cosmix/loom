//! Agent type from the `agent-<id>.meta.json` sidecar Claude Code writes beside
//! a subagent transcript. It ranks below loom's hook ledger:
//! `ParentSpawns::fill_fallbacks` asks here only for a field the ledger left
//! empty.

use std::path::Path;

use serde_json::Value;

use crate::fs::safe_read::read_bounded;

/// A sidecar is a few keys; anything past this is not one.
const MAX_SIDECAR_BYTES: usize = 64 * 1024;

/// What `ParentSpawns::fill_fallbacks` takes from one sidecar read.
pub(super) struct Sidecar {
    /// `spawnedAgentType`, else `agentType`. A team agent's `agentType` is the
    /// teammate's name, so `spawnedAgentType` wins when both are present.
    pub(super) agent_type: Option<String>,
    /// The parent's spawn `tool_use` id.
    pub(super) tool_use_id: Option<String>,
    /// The model the spawn requested, when the sidecar records one.
    pub(super) model: Option<String>,
}

/// The sidecar beside the subagent transcript at `transcript_path`, read once
/// within `MAX_SIDECAR_BYTES`. Any read or parse failure, including a
/// symlinked sidecar, yields `None`.
pub(super) fn read_sidecar(transcript_path: &Path) -> Option<Sidecar> {
    let dir = transcript_path.parent()?;
    let stem = transcript_path.file_stem()?.to_str()?;
    let name = format!("{stem}.meta.json");
    let bytes = read_bounded(dir, Path::new(&name), MAX_SIDECAR_BYTES).ok()?;
    let meta: Value = serde_json::from_slice(&bytes).ok()?;
    let text = |key: &str| {
        meta.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Some(Sidecar {
        agent_type: text("spawnedAgentType").or_else(|| text("agentType")),
        tool_use_id: text("toolUseId"),
        model: text("model"),
    })
}

/// The sidecar's agent type for the subagent transcript at `transcript_path`.
/// Production code reads the whole [`Sidecar`] once; only the tests ask for the
/// agent type alone.
#[cfg(test)]
pub(super) fn agent_type_from_sidecar(transcript_path: &Path) -> Option<String> {
    read_sidecar(transcript_path)?.agent_type
}

#[cfg(test)]
mod tests;
