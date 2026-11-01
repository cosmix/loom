//! Bounded live deduplication and the attribution required to retract revisions.
use super::model::{TokenUsage, BUCKET_SECS, UNKNOWN_MODEL};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

pub(super) const MESSAGE_TABLE_CAP: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Observation {
    pub stamp: DateTime<Utc>,
    pub bucket: i64,
    pub usage: TokenUsage,
    pub model: String,
    pub effort: Option<String>,
}

impl Observation {
    pub fn new(
        stamp: DateTime<Utc>,
        usage: TokenUsage,
        model: &Option<String>,
        effort: &Option<String>,
    ) -> Self {
        Self {
            stamp,
            bucket: stamp.timestamp().div_euclid(BUCKET_SECS),
            usage,
            model: model.clone().unwrap_or_else(|| UNKNOWN_MODEL.to_owned()),
            effort: effort.clone(),
        }
    }
}

pub(super) fn subtract(total: &mut TokenUsage, old: &TokenUsage) {
    total.input = total.input.saturating_sub(old.input);
    total.cache_write = total.cache_write.saturating_sub(old.cache_write);
    total.cache_read = total.cache_read.saturating_sub(old.cache_read);
    total.output = total.output.saturating_sub(old.output);
}

/// The selected usage observation per Claude `message.id`, for the most
/// recent [`MESSAGE_TABLE_CAP`] ids, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<SavedMessage>", into = "Vec<SavedMessage>")]
pub(super) struct MessageTable {
    pub(super) order: VecDeque<String>,
    pub(super) observations: HashMap<String, Observation>,
}

impl MessageTable {
    pub(super) fn is_stale(&self, id: Option<&str>, stamp: DateTime<Utc>) -> bool {
        id.and_then(|id| self.observations.get(id))
            .is_some_and(|previous| !crate::provider_data::supersedes(previous.stamp, stamp))
    }

    /// Select an already-validated observation and return the one it replaces.
    pub(super) fn replace(&mut self, id: String, observation: Observation) -> Option<Observation> {
        if !self.observations.contains_key(&id) {
            self.remember(&id);
        }
        self.observations.insert(id, observation)
    }

    /// Appends a new id, evicting the oldest one past the cap.
    fn remember(&mut self, id: &str) {
        self.order.push_back(id.to_owned());
        if self.order.len() > MESSAGE_TABLE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.observations.remove(&oldest);
            }
        }
    }
}

type SavedMessage = (String, Observation);

impl From<Vec<SavedMessage>> for MessageTable {
    fn from(saved: Vec<SavedMessage>) -> Self {
        let mut table = Self::default();
        for (id, observation) in saved {
            table.replace(id, observation);
        }
        table
    }
}

impl From<MessageTable> for Vec<SavedMessage> {
    fn from(mut table: MessageTable) -> Self {
        table
            .order
            .into_iter()
            .filter_map(|id| {
                table
                    .observations
                    .remove(&id)
                    .map(|observation| (id, observation))
            })
            .collect()
    }
}
