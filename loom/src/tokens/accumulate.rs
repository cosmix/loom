//! Per-agent token state: totals, 30 s buckets, and where reading the agent's
//! one file stopped, with the dedupe state needed to continue from there.

use super::claude_line::{self, ClaudeLine};
use super::codex_line::{self, CodexLine, CodexTotals};
use super::model::{
    AgentKind, ModelUsage, TokenUsage, BUCKET_SECS, MAX_MODEL_KEYS, OTHER_MODEL, UNKNOWN_MODEL,
};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
use crate::context::untrusted::inline_safe;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Message ids remembered per Claude transcript; older ids are evicted.
pub(super) const MESSAGE_TABLE_CAP: usize = 4096;

/// A stage's agents by agent key.
pub(super) type StageAgents = BTreeMap<String, AgentState>;

/// One agent's accumulated usage and the state of reading its file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct AgentState {
    pub(super) kind: AgentKind,
    pub(super) agent_type: Option<String>,
    pub(super) model: Option<String>,
    /// The last reasoning effort a line named.
    pub(super) effort: Option<String>,
    /// Line timestamp of the first usage read from the file.
    pub(super) first_seen: Option<i64>,
    pub(super) total: TokenUsage,
    /// `total` split by (model, effort): at most [`MAX_MODEL_KEYS`] keys plus
    /// one `(OTHER_MODEL, None)` key that absorbs the rest.
    pub(super) by_model: Vec<ModelUsage>,
    /// Usage per bucket index (`unix_secs / BUCKET_SECS`).
    pub(super) buckets: BTreeMap<i64, TokenUsage>,
    pub(super) unreadable: u64,
    pub(super) file: FileCursor,
}

/// The agent's file, how far it was read, and what dedupe needs to go on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct FileCursor {
    pub(super) path: String,
    pub(super) inode: u64,
    /// Byte offset just past the last complete line read.
    pub(super) offset: u64,
    pub(super) messages: MessageTable,
    /// The last Codex `total_token_usage` read.
    pub(super) codex_total: Option<SavedTotals>,
}

/// Serializable copy of [`CodexTotals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SavedTotals {
    input: u64,
    cached_input: u64,
    cache_write: u64,
    output: u64,
}

impl From<CodexTotals> for SavedTotals {
    fn from(totals: CodexTotals) -> Self {
        Self {
            input: totals.input,
            cached_input: totals.cached_input,
            cache_write: totals.cache_write,
            output: totals.output,
        }
    }
}

impl From<SavedTotals> for CodexTotals {
    fn from(saved: SavedTotals) -> Self {
        Self {
            input: saved.input,
            cached_input: saved.cached_input,
            cache_write: saved.cache_write,
            output: saved.output,
        }
    }
}

/// A message id with its usage as `[input, cache_write, cache_read, output]`,
/// the compact form a checkpoint stores.
type SavedMessage = (String, [u64; 4]);

/// The field-wise maximum usage seen per Claude `message.id`, for the most
/// recent [`MESSAGE_TABLE_CAP`] ids, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<SavedMessage>", into = "Vec<SavedMessage>")]
pub(super) struct MessageTable {
    order: VecDeque<String>,
    max: HashMap<String, TokenUsage>,
}

impl MessageTable {
    /// Records `usage` into the field-wise maximum for `id` and returns how far
    /// it exceeds what was recorded before: all of it for an id not seen yet.
    pub(super) fn increase(&mut self, id: &str, usage: &TokenUsage) -> TokenUsage {
        let seen = match self.max.get(id) {
            Some(seen) => *seen,
            None => {
                self.remember(id);
                TokenUsage::default()
            }
        };
        let gain = TokenUsage {
            input: usage.input.saturating_sub(seen.input),
            cache_write: usage.cache_write.saturating_sub(seen.cache_write),
            cache_read: usage.cache_read.saturating_sub(seen.cache_read),
            output: usage.output.saturating_sub(seen.output),
        };
        let mut max = seen;
        max.add(&gain);
        self.max.insert(id.to_owned(), max);
        gain
    }

    /// Appends a new id, evicting the oldest one past the cap.
    fn remember(&mut self, id: &str) {
        self.order.push_back(id.to_owned());
        if self.order.len() > MESSAGE_TABLE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.max.remove(&oldest);
            }
        }
    }
}

impl From<Vec<SavedMessage>> for MessageTable {
    fn from(saved: Vec<SavedMessage>) -> Self {
        let mut table = Self::default();
        for (id, [input, cache_write, cache_read, output]) in saved {
            let usage = TokenUsage {
                input,
                cache_write,
                cache_read,
                output,
            };
            table.increase(&id, &usage);
        }
        table
    }
}

impl From<MessageTable> for Vec<SavedMessage> {
    fn from(table: MessageTable) -> Self {
        let MessageTable { order, mut max } = table;
        order
            .into_iter()
            .map(|id| {
                let usage = max.remove(&id).unwrap_or_default();
                let fields = [
                    usage.input,
                    usage.cache_write,
                    usage.cache_read,
                    usage.output,
                ];
                (id, fields)
            })
            .collect()
    }
}

impl AgentState {
    pub(super) fn new(kind: AgentKind, path: &str) -> Self {
        Self {
            kind,
            agent_type: None,
            model: None,
            effort: None,
            first_seen: None,
            total: TokenUsage::default(),
            by_model: Vec::new(),
            buckets: BTreeMap::new(),
            unreadable: 0,
            file: FileCursor {
                path: path.to_owned(),
                ..FileCursor::default()
            },
        }
    }

    /// Forgets everything read from the file, to read `path` (inode `inode`)
    /// again from offset 0. The ledger's agent type is kept.
    pub(super) fn restart(&mut self, path: &str, inode: u64) {
        let agent_type = self.agent_type.take();
        *self = Self {
            agent_type,
            ..Self::new(self.kind, path)
        };
        self.file.inode = inode;
    }

    /// Adds one complete line of the agent's file.
    pub(super) fn ingest(&mut self, line: &str) {
        match self.kind {
            AgentKind::Main | AgentKind::Subagent => self.ingest_claude(line),
            AgentKind::Codex => self.ingest_codex(line),
        }
    }

    /// Counts a line skipped unread, as one that fails to parse is counted.
    pub(super) fn count_unreadable(&mut self) {
        self.unreadable = self.unreadable.saturating_add(1);
    }

    /// Drops buckets before bucket index `oldest`; their usage stays in `total`.
    pub(super) fn prune(&mut self, oldest: i64) {
        self.buckets.retain(|bucket, _| *bucket >= oldest);
    }

    fn ingest_claude(&mut self, line: &str) {
        match claude_line::parse_line(line) {
            ClaudeLine::Usage {
                message_id,
                timestamp,
                model,
                effort,
                usage,
            } => {
                // An API-error row names the `<synthetic>` sentinel, not a model. A
                // value that sanitizes to nothing leaves the current one, as a line
                // without it does.
                if let Some(model) = model
                    .filter(|model| model != SYNTHETIC_MODEL)
                    .and_then(|model| visible(&model))
                {
                    self.model = Some(model);
                }
                if let Some(effort) = effort.and_then(|effort| visible(&effort)) {
                    self.effort = Some(effort);
                }
                let spent = match message_id {
                    Some(id) => self.file.messages.increase(&id, &usage),
                    None => usage,
                };
                self.add(timestamp, &spent);
            }
            ClaudeLine::Unreadable => self.count_unreadable(),
            ClaudeLine::Other => {}
        }
    }

    /// Counts the increase over the last cumulative total; a total with any
    /// field below the last one is a new baseline and counts in full.
    fn ingest_codex(&mut self, line: &str) {
        match codex_line::parse_line(line) {
            CodexLine::TokenCount { timestamp, total } => {
                let delta = self
                    .file
                    .codex_total
                    .and_then(|last| total.delta_since(&last.into()))
                    .unwrap_or(total);
                self.file.codex_total = Some(total.into());
                self.add(timestamp, &delta.to_usage());
            }
            CodexLine::TurnContext { model, effort } => {
                self.model = visible(&model);
                self.effort = effort.as_deref().and_then(visible);
            }
            CodexLine::Unreadable => self.count_unreadable(),
            CodexLine::SessionMeta { .. } | CodexLine::Other => {}
        }
    }

    /// Adds usage to the total and to the bucket of the line's own timestamp.
    fn add(&mut self, timestamp: i64, usage: &TokenUsage) {
        if *usage == TokenUsage::default() {
            return;
        }
        self.first_seen.get_or_insert(timestamp);
        self.total.add(usage);
        self.buckets
            .entry(timestamp.div_euclid(BUCKET_SECS))
            .or_default()
            .add(usage);
        self.add_model_usage(usage);
    }

    /// Books `usage` under the agent's current (model, effort); a key past
    /// [`MAX_MODEL_KEYS`] goes to the `(OTHER_MODEL, None)` entry.
    fn add_model_usage(&mut self, usage: &TokenUsage) {
        let model = self.model.as_deref().unwrap_or(UNKNOWN_MODEL);
        let effort = self.effort.as_deref();
        let same = |entry: &ModelUsage| entry.model == model && entry.effort.as_deref() == effort;
        if let Some(entry) = self.by_model.iter_mut().find(|entry| same(entry)) {
            entry.total.add(usage);
            return;
        }
        let is_other = |entry: &ModelUsage| entry.model == OTHER_MODEL && entry.effort.is_none();
        let keys = self
            .by_model
            .iter()
            .filter(|entry| !is_other(entry))
            .count();
        if keys < MAX_MODEL_KEYS {
            self.by_model.push(ModelUsage {
                model: model.to_owned(),
                effort: effort.map(str::to_owned),
                total: *usage,
            });
        } else if let Some(other) = self.by_model.iter_mut().find(|entry| is_other(entry)) {
            other.total.add(usage);
        } else {
            self.by_model.push(ModelUsage {
                model: OTHER_MODEL.to_owned(),
                effort: None,
                total: *usage,
            });
        }
    }
}

/// `value` made inline-safe, or `None` when nothing is left of it: a blank model or
/// effort would show as an empty label and collide with an absent one.
fn visible(value: &str) -> Option<String> {
    Some(inline_safe(value)).filter(|safe| !safe.is_empty())
}

#[cfg(test)]
#[path = "accumulate_tests.rs"]
mod tests;
