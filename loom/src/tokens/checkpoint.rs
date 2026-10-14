//! Tracker state between passes, kept in `.loom/work/tokens/checkpoint.json`:
//! every agent's totals, buckets and file cursor, and the Codex rollout lookups.

use super::accumulate::{AgentState, MessageTable, StageAgents};
use super::discover::{Source, ThreadLookup};
use super::tracker::CHECKPOINT_FILE;
use crate::fs::safe_read::{is_not_found, read_bounded, OverLimit};
use crate::quota::cache::atomic_write;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub(super) const CHECKPOINT_VERSION: u32 = 1;
pub(super) const MAX_CHECKPOINT_BYTES: usize = 8 * 1024 * 1024;

/// What [`Checkpoint::save`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Saved {
    /// The checkpoint file was replaced.
    Written,
    /// Nothing was written: the compact JSON is this many bytes, over
    /// [`MAX_CHECKPOINT_BYTES`], which `load` would refuse to read back.
    OverCap(usize),
}

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
    /// a FIFO is ever read through. An empty checkpoint that is not just a
    /// first run comes with the reason, for the pass's warning.
    pub(super) fn load_reporting(tokens_dir: &Path) -> (Self, Option<String>) {
        match Self::try_load(tokens_dir) {
            Ok(Some(checkpoint)) => (checkpoint, None),
            Ok(None) => (Self::empty(), None),
            Err(reason) => {
                let warning = format!("{reason}; every transcript is read from the start");
                (Self::empty(), Some(warning))
            }
        }
    }

    /// The checkpoint, `None` when there is none yet, or why it is unusable.
    fn try_load(tokens_dir: &Path) -> Result<Option<Self>, String> {
        let file = Path::new(CHECKPOINT_FILE);
        let bytes = match read_bounded(tokens_dir, file, MAX_CHECKPOINT_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if is_not_found(&error) => return Ok(None),
            Err(error) if error.is::<OverLimit>() => {
                return Err(format!("checkpoint is over {MAX_CHECKPOINT_BYTES} bytes"));
            }
            Err(error) => return Err(format!("checkpoint is unreadable ({error:#})")),
        };
        let checkpoint = serde_json::from_slice::<Self>(&bytes)
            .map_err(|error| format!("checkpoint is unparseable ({error})"))?;
        if checkpoint.version != CHECKPOINT_VERSION {
            return Err(format!("checkpoint is version {}", checkpoint.version));
        }
        Ok(Some(checkpoint))
    }

    /// [`Self::load_reporting`] without the reason.
    #[cfg(test)]
    pub(super) fn load(tokens_dir: &Path) -> Self {
        Self::load_reporting(tokens_dir).0
    }

    /// Writes the checkpoint atomically as compact JSON, unless that is over
    /// [`MAX_CHECKPOINT_BYTES`]: the next `load` would refuse such a file and
    /// rescan every transcript from the start.
    pub(super) fn save(&self, tokens_dir: &Path) -> Result<Saved> {
        let json = serde_json::to_string(self).context("serializing the token checkpoint")?;
        if json.len() > MAX_CHECKPOINT_BYTES {
            return Ok(Saved::OverCap(json.len()));
        }
        atomic_write(&tokens_dir.join(CHECKPOINT_FILE), &json)
            .context("writing the token checkpoint")?;
        Ok(Saved::Written)
    }

    /// Empties the message-id table of every agent not in `keep`, given as
    /// `(stage id, agent key)`. A later line of a cleared file that repeats an
    /// id counts it again.
    pub(super) fn clear_tables_except(&mut self, keep: &HashSet<(&str, &str)>) {
        for (stage_id, agents) in &mut self.stages {
            for (key, agent) in agents {
                if !keep.contains(&(stage_id.as_str(), key.as_str())) {
                    agent.file.messages = MessageTable::default();
                }
            }
        }
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
