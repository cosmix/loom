//! Partition of a [`WebSnapshot`] into the parts the `/ws` delta protocol sends
//! as separate messages, and the comparisons that decide which messages a new
//! collect needs.
//!
//! A snapshot is serialized once into a JSON value and sorted into: immutable
//! stage keys (sent only in a `full`), clock values (sent in a `tick`),
//! everything else per stage (sent whole in a `state`), and the top-level keys.
//! Anything not named by a constant below falls into the safe default: an
//! unknown stage key is dynamic, an unknown top-level or status key forces a
//! `full` when it changes. `feed_split_tests.rs` fails when a new wire key is
//! not classified here, so the defaults are a fallback, not a design.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};

use super::model::WebSnapshot;

#[cfg(test)]
#[path = "feed_split_tests.rs"]
mod tests;

/// Stage keys that never change while the stage id exists; sent in `full` only.
/// `web/src/api/frames.ts` (`STAGE_IMMUTABLE_MASK`) lists the same keys.
pub(super) const IMMUTABLE_STAGE_KEYS: [&str; 7] = [
    "id",
    "name",
    "description",
    "summary",
    "stage_type",
    "dependencies",
    "max_retries",
];

/// Stage keys that advance with the wall clock, with their `tick` entry names.
/// `contract_testing.running_secs` is the fifth clock value (`contract`).
pub(super) const CLOCK_STAGE_KEYS: [(&str, &str); 4] = [
    ("elapsed_secs", "elapsed"),
    ("execution_secs", "exec"),
    ("staleness_secs", "stale"),
    ("judge_heartbeat_secs", "judge"),
];

/// Top-level keys sent in a `state` message when they change.
pub(super) const STATE_TOP_KEYS: [&str; 5] = ["attention", "alerts", "daemon", "source", "notice"];

/// `status` keys sent in a `state` message when they change.
pub(super) const STATE_STATUS_KEYS: [&str; 5] =
    ["merge", "progress", "plan_name", "quota", "activity"];

/// Top-level keys that carry the clock of a `tick`.
#[cfg(test)]
pub(super) const CLOCK_TOP_KEYS: [&str; 2] = ["generated_at", "tick_age_secs"];

/// Top-level keys whose change forces a `full`.
#[cfg(test)]
pub(super) const FULL_ONLY_KEYS: [&str; 3] = ["terminals", "version", "busy"];

/// Stands in for a numeric clock value when comparing, so a count moving from
/// one number to another is invisible while null to number is a change.
const MASK: &str = "#";

/// The numeric clock values of one stage, as a `tick` entry carries them.
#[derive(Serialize, Default, Clone, PartialEq, Debug)]
struct ClockEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    judge: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<i64>,
}

impl ClockEntry {
    fn of(dynamic: &Map<String, Value>) -> Self {
        let [elapsed, exec, stale, judge] =
            CLOCK_STAGE_KEYS.map(|(key, _)| dynamic.get(key).and_then(Value::as_i64));
        let contract = dynamic
            .get("contract_testing")
            .and_then(|testing| testing.get("running_secs"))
            .and_then(Value::as_i64);
        Self {
            elapsed,
            exec,
            stale,
            judge,
            contract,
        }
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One stage, split by how its keys change.
struct StagePart {
    id: String,
    immutable: Map<String, Value>,
    /// Every non-immutable key, clock values included.
    dynamic: Map<String, Value>,
    /// `dynamic` with the numeric clock values masked, for change detection.
    masked: Value,
    clock: ClockEntry,
}

impl StagePart {
    fn of(value: Value) -> Result<Self> {
        let Value::Object(object) = value else {
            bail!("dashboard stage is not a JSON object");
        };
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .context("dashboard stage has no id")?
            .to_owned();
        let (immutable, dynamic): (Map<String, Value>, Map<String, Value>) = object
            .into_iter()
            .partition(|(key, _)| IMMUTABLE_STAGE_KEYS.contains(&key.as_str()));
        let mut masked = Value::Object(dynamic.clone());
        mask_clock(&mut masked, &CLOCK_STAGE_KEYS.map(|(key, _)| key));
        if let Some(testing) = masked.get_mut("contract_testing") {
            mask_clock(testing, &["running_secs"]);
        }
        let clock = ClockEntry::of(&dynamic);
        Ok(Self {
            id,
            immutable,
            dynamic,
            masked,
            clock,
        })
    }
}

/// A snapshot sorted into the parts the delta messages carry.
pub(super) struct Split {
    stages: Vec<StagePart>,
    /// `STATE_TOP_KEYS` values, with `notice` as null when absent.
    top: Map<String, Value>,
    /// `top` with each attention entry's judge heartbeat masked.
    top_masked: Map<String, Value>,
    /// `STATE_STATUS_KEYS` values.
    status: Map<String, Value>,
    /// The token flow, null when absent.
    pub(super) tokens: Value,
    /// Everything else: a change forces a `full`.
    full_only: Map<String, Value>,
    pub(super) at: Value,
    pub(super) tick_age: Value,
}

impl Split {
    pub(super) fn of(snapshot: &WebSnapshot) -> Result<Self> {
        let Value::Object(mut root) =
            serde_json::to_value(snapshot).context("serialize dashboard snapshot")?
        else {
            bail!("dashboard snapshot is not a JSON object");
        };
        let Some(Value::Object(mut status)) = root.remove("status") else {
            bail!("dashboard snapshot has no status object");
        };
        let Some(Value::Array(items)) = status.remove("stages") else {
            bail!("dashboard snapshot has no stage list");
        };
        let stages = items
            .into_iter()
            .map(StagePart::of)
            .collect::<Result<Vec<_>>>()?;
        let tokens = status.remove("tokens").unwrap_or(Value::Null);
        let at = root.remove("generated_at").unwrap_or(Value::Null);
        let tick_age = root.remove("tick_age_secs").unwrap_or(Value::Null);
        let mut top = take_keys(&mut root, &STATE_TOP_KEYS);
        top.entry("notice").or_insert(Value::Null);
        let mut top_masked = top.clone();
        if let Some(Value::Array(entries)) = top_masked.get_mut("attention") {
            for entry in entries {
                mask_clock(entry, &["judge_heartbeat_secs"]);
            }
        }
        let state_status = take_keys(&mut status, &STATE_STATUS_KEYS);
        // Unclassified keys keep the protocol correct: their change is a `full`.
        let mut full_only = root;
        full_only.extend(
            status
                .into_iter()
                .map(|(key, value)| (format!("status.{key}"), value)),
        );
        Ok(Self {
            stages,
            top,
            top_masked,
            status: state_status,
            tokens,
            full_only,
            at,
            tick_age,
        })
    }
}

fn take_keys(from: &mut Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    keys.iter()
        .filter_map(|key| from.remove(*key).map(|value| ((*key).to_owned(), value)))
        .collect()
}

/// Replace each numeric value under `keys` with a fixed marker; null stays null.
fn mask_clock(value: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(slot) = value.get_mut(*key) {
            if slot.is_number() {
                *slot = Value::String(MASK.to_owned());
            }
        }
    }
}

/// Whether the change from `prev` to `next` cannot be sent as deltas: a stage
/// was added, removed or reordered, an immutable stage key moved, or a
/// full-only key changed.
pub(super) fn needs_full(prev: &Split, next: &Split) -> bool {
    prev.stages.len() != next.stages.len()
        || prev
            .stages
            .iter()
            .zip(&next.stages)
            .any(|(a, b)| a.id != b.id || a.immutable != b.immutable)
        || prev.full_only != next.full_only
}

/// The parts of a `state` message: changed stages (whole dynamic part), changed
/// `status` keys, and changed top-level keys (`notice` as null once cleared).
/// Callers have already ruled out [`needs_full`].
pub(super) fn state_changes(
    prev: &Split,
    next: &Split,
) -> (Map<String, Value>, Map<String, Value>, Map<String, Value>) {
    let stages = prev
        .stages
        .iter()
        .zip(&next.stages)
        .filter(|(a, b)| a.masked != b.masked)
        .map(|(_, b)| (b.id.clone(), Value::Object(b.dynamic.clone())))
        .collect();
    let status = STATE_STATUS_KEYS
        .iter()
        .filter(|key| prev.status.get(**key) != next.status.get(**key))
        .map(|key| {
            let value = next.status.get(*key).cloned().unwrap_or(Value::Null);
            ((*key).to_owned(), value)
        })
        .collect();
    let top = STATE_TOP_KEYS
        .iter()
        .filter(|key| prev.top_masked.get(**key) != next.top_masked.get(**key))
        .filter_map(|key| {
            next.top
                .get(*key)
                .map(|value| ((*key).to_owned(), value.clone()))
        })
        .collect();
    (stages, status, top)
}

/// The `stages` of a `tick`: each stage whose clock entry moved and holds at
/// least one number.
pub(super) fn tick_stages(prev: &Split, next: &Split) -> Result<Map<String, Value>> {
    let mut stages = Map::new();
    for (a, b) in prev.stages.iter().zip(&next.stages) {
        if a.clock != b.clock && !b.clock.is_empty() {
            let entry = serde_json::to_value(&b.clock).context("serialize clock entry")?;
            stages.insert(b.id.clone(), entry);
        }
    }
    Ok(stages)
}
