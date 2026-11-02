//! Agent type from the `agent-<id>.meta.json` sidecar Claude Code writes beside
//! a subagent transcript. It ranks below loom's hook ledger: `parse_all` asks
//! here only when the ledger has no row.

use std::path::Path;

use serde_json::Value;

use crate::fs::safe_read::read_bounded;

/// A sidecar is a few keys; anything past this is not one.
const MAX_SIDECAR_BYTES: usize = 64 * 1024;

/// The sidecar's `spawnedAgentType`, else its `agentType`, for the subagent
/// transcript at `transcript_path`. A team agent's `agentType` is the
/// teammate's name, so `spawnedAgentType` wins when both are present. Any read
/// or parse failure, including a symlinked sidecar, yields `None`.
pub(super) fn agent_type_from_sidecar(transcript_path: &Path) -> Option<String> {
    let dir = transcript_path.parent()?;
    let stem = transcript_path.file_stem()?.to_str()?;
    let name = format!("{stem}.meta.json");
    let bytes = read_bounded(dir, Path::new(&name), MAX_SIDECAR_BYTES).ok()?;
    let meta: Value = serde_json::from_slice(&bytes).ok()?;
    ["spawnedAgentType", "agentType"]
        .into_iter()
        .filter_map(|key| meta.get(key)?.as_str())
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
