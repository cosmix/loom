//! Tracker state between passes, kept in `.loom/work/tokens/checkpoint.json`:
//! every agent's totals, buckets and file cursor, and the Codex rollout lookups.

use super::accumulate::{AgentState, StageAgents};
use super::discover::{Source, ThreadLookup};
use super::tracker::CHECKPOINT_FILE;
use crate::fs::safe_read::read_bounded;
use crate::quota::cache::atomic_write;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) const CHECKPOINT_VERSION: u32 = 1;
pub(super) const MAX_CHECKPOINT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Checkpoint {
    pub(super) version: u32,
    /// Agents by stage id, then by agent key.
    pub(super) stages: BTreeMap<String, StageAgents>,
    /// Codex rollout lookups by thread id.
    pub(super) threads: BTreeMap<String, ThreadLookup>,
}

impl Checkpoint {
    /// No state: the next pass reads every file from the start.
    pub(super) fn empty() -> Self {
        Self {
            version: CHECKPOINT_VERSION,
            stages: BTreeMap::new(),
            threads: BTreeMap::new(),
        }
    }

    /// The checkpoint in `tokens_dir`, or [`Self::empty`] when it is missing,
    /// a symlink, not a regular file, over [`MAX_CHECKPOINT_BYTES`],
    /// unparseable, or of another version. `read_bounded` opens no-follow and
    /// non-blocking and checks the size before reading, so neither a link nor
    /// a FIFO is ever read through.
    pub(super) fn load(tokens_dir: &Path) -> Self {
        read_bounded(tokens_dir, Path::new(CHECKPOINT_FILE), MAX_CHECKPOINT_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|checkpoint| checkpoint.version == CHECKPOINT_VERSION)
            .unwrap_or_else(Self::empty)
    }

    /// Writes the checkpoint atomically as compact JSON.
    pub(super) fn save(&self, tokens_dir: &Path) -> Result<()> {
        let json = serde_json::to_string(self).context("serializing the token checkpoint")?;
        atomic_write(&tokens_dir.join(CHECKPOINT_FILE), &json)
            .context("writing the token checkpoint")
    }

    /// The state of `source`'s agent, created empty when new.
    pub(super) fn agent(&mut self, source: &Source) -> &mut AgentState {
        self.stages
            .entry(source.stage_id.clone())
            .or_default()
            .entry(source.key.clone())
            .or_insert_with(|| AgentState::new(source.kind, &source.path))
    }

    /// Drops every agent's buckets before bucket index `oldest`.
    pub(super) fn prune(&mut self, oldest: i64) {
        for agent in self
            .stages
            .values_mut()
            .flat_map(|agents| agents.values_mut())
        {
            agent.prune(oldest);
        }
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
