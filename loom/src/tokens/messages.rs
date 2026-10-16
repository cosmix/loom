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
        line: &str,
        timestamp: i64,
        usage: TokenUsage,
        model: &Option<String>,
        effort: &Option<String>,
    ) -> Self {
        let stamp = stamp(line);
        Self {
            stamp,
            bucket: timestamp.div_euclid(BUCKET_SECS),
            usage,
            model: model.clone().unwrap_or_else(|| UNKNOWN_MODEL.to_owned()),
            effort: effort.clone(),
        }
    }
}

fn stamp(line: &str) -> DateTime<Utc> {
    let value: serde_json::Value = serde_json::from_str(line).expect("parsed Claude line");
    DateTime::parse_from_rfc3339(value["timestamp"].as_str().unwrap())
        .expect("validated timestamp")
        .with_timezone(&Utc)
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
    pub(super) max: HashMap<String, TokenUsage>,
    pub(super) observations: HashMap<String, Observation>,
}

impl MessageTable {
    pub(super) fn is_stale(&self, id: Option<&str>, line: &str) -> bool {
        id.and_then(|id| self.observations.get(id))
            .is_some_and(|previous| !crate::provider_data::supersedes(previous.stamp, stamp(line)))
    }

    /// Records the selected vector and returns its positive change.
    /// Retractions are applied by the live accumulator before replacement.
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
        self.max.insert(id.to_owned(), *usage);
        gain
    }

    /// Appends a new id, evicting the oldest one past the cap.
    fn remember(&mut self, id: &str) {
        self.order.push_back(id.to_owned());
        if self.order.len() > MESSAGE_TABLE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.max.remove(&oldest);
                self.observations.remove(&oldest);
            }
        }
    }
}

type SavedMessage = (String, TokenUsage, Option<Observation>);

impl From<Vec<SavedMessage>> for MessageTable {
    fn from(saved: Vec<SavedMessage>) -> Self {
        let mut table = Self::default();
        for (id, usage, observation) in saved {
            table.increase(&id, &usage);
            if let Some(observation) = observation {
                table.observations.insert(id, observation);
            }
        }
        table
    }
}

impl From<MessageTable> for Vec<SavedMessage> {
    fn from(mut table: MessageTable) -> Self {
        table
            .order
            .into_iter()
            .map(|id| {
                let usage = table.max.remove(&id).unwrap_or_default();
                let observation = table.observations.remove(&id);
                (id, usage, observation)
            })
            .collect()
    }
}
