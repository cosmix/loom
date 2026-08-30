//! Reads the optional hook-side ledgers that add spawn metadata the Claude
//! transcript format does not carry. Every lookup is deliberately best-effort:
//! this read-only command remains useful before hooks have created a ledger.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Return the agent type recorded for `agent_id`, if the hook-side ledgers
/// can establish one without guessing. Stage ids are unknown to this command,
/// so every stage directory is considered just as termination lookup does.
pub(crate) fn agent_type(work_dir: Option<&Path>, agent_id: &str) -> Option<String> {
    let stage_dirs = stage_directories(work_dir)?;
    match starts_agent_type(&stage_dirs, agent_id) {
        Some(agent_type) => agent_type,
        None => spawns_agent_type(&stage_dirs),
    }
}

/// Resolve one transcript's agent type from an actual SubagentStart row.
///
/// New rows carry Claude's parent transcript UUID, which prevents historical records
/// with a colliding agent id from being joined to the wrong transcript. Old
/// rows predate any session field; they remain usable only when every matching
/// row agrees. The ambiguous `session_id` field from a short-lived development
/// schema held Loom's unrelated session id and is never treated as legacy or
/// join evidence. The older `spawns.jsonl` has no agent id at all and is
/// therefore never evidence for usage attribution.
#[cfg(test)]
pub(crate) fn started_agent_type(
    work_dir: Option<&Path>,
    agent_id: &str,
    parent_session_id: &str,
) -> Option<String> {
    StartedAgentTypeIndex::load(work_dir).get(agent_id, parent_session_id)
}

/// An in-memory index over hook-written `SubagentStart` rows, used by
/// `loom usage` to avoid reopening every `starts.jsonl` for every transcript.
///
/// The index deliberately contains no `spawns.jsonl` data: those entries do
/// not identify an individual agent. `agent_type()` retains that older,
/// interactive command's fallback separately.
#[derive(Default)]
pub(crate) struct StartedAgentTypeIndex {
    /// A scoped start is addressable only by Claude's parent transcript id
    /// and the agent id together. `Unknown` covers conflicting nonempty rows;
    /// empty types are ignored for agreement but still suppress legacy joins.
    scoped: HashMap<(String, String), AgentTypeAgreement>,
    /// Any scoped row (including the obsolete ambiguous `session_id` schema)
    /// makes an unscoped legacy row for this agent id unsafe to use.
    scoped_agents: HashSet<String>,
    /// Pre-session-schema starts can be used only when all rows for the id
    /// agree and there are no scoped rows for that id.
    legacy: HashMap<String, AgentTypeAgreement>,
}

#[derive(Clone)]
enum AgentTypeAgreement {
    Known(String),
    Unknown,
}

impl AgentTypeAgreement {
    fn value(&self) -> Option<String> {
        match self {
            Self::Known(value) => Some(value.clone()),
            Self::Unknown => None,
        }
    }
}

impl StartedAgentTypeIndex {
    pub(crate) fn load(work_dir: Option<&Path>) -> Self {
        let Some(stage_dirs) = stage_directories(work_dir) else {
            return Self::default();
        };

        let mut index = Self::default();
        for stage_dir in stage_dirs {
            for entry in json_lines(&stage_dir.join("starts.jsonl")) {
                index.record(&entry);
            }
        }
        index
    }

    pub(crate) fn get(&self, agent_id: &str, parent_session_id: &str) -> Option<String> {
        let key = (parent_session_id.to_owned(), agent_id.to_owned());
        if let Some(agreement) = self.scoped.get(&key) {
            return agreement.value();
        }
        if self.scoped_agents.contains(agent_id) {
            return None;
        }
        self.legacy
            .get(agent_id)
            .and_then(AgentTypeAgreement::value)
    }

    fn record(&mut self, entry: &Value) {
        let Some(agent_id) = entry.get("agent_id").and_then(Value::as_str) else {
            return;
        };
        let agent_type = entry
            .get("agent_type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());

        match entry.get("parent_session_id") {
            Some(parent_session_id) if parent_session_id.as_str().is_some() => {
                let parent_session_id = parent_session_id.as_str().expect("checked above");
                self.scoped_agents.insert(agent_id.to_owned());
                if let Some(agent_type) = agent_type {
                    record_agreement(
                        self.scoped
                            .entry((parent_session_id.to_owned(), agent_id.to_owned()))
                            .or_insert_with(|| AgentTypeAgreement::Known(agent_type.to_owned())),
                        Some(agent_type),
                    );
                }
            }
            // A malformed scoped row cannot be joined, but must still keep
            // an otherwise matching legacy row from being guessed at.
            Some(_) => {
                self.scoped_agents.insert(agent_id.to_owned());
            }
            // `session_id` was Loom's own session id in an old schema. It is
            // deliberately neither an exact match nor a usable legacy row.
            None if entry.get("session_id").is_some() => {
                self.scoped_agents.insert(agent_id.to_owned());
            }
            None => {
                if let Some(agent_type) = agent_type {
                    record_agreement(
                        self.legacy
                            .entry(agent_id.to_owned())
                            .or_insert_with(|| AgentTypeAgreement::Known(agent_type.to_owned())),
                        Some(agent_type),
                    );
                }
            }
        }
    }
}

fn record_agreement(agreement: &mut AgentTypeAgreement, value: Option<&str>) {
    let Some(value) = value else {
        *agreement = AgentTypeAgreement::Unknown;
        return;
    };
    match agreement {
        AgentTypeAgreement::Known(known) if known == value => {}
        AgentTypeAgreement::Known(_) => *agreement = AgentTypeAgreement::Unknown,
        AgentTypeAgreement::Unknown => {}
    }
}

fn stage_directories(work_dir: Option<&Path>) -> Option<Vec<PathBuf>> {
    let work_dir = work_dir?;
    let entries = fs::read_dir(work_dir.join("subagents")).ok()?;
    Some(
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
    )
}

/// `Some(None)` means a start row identified the agent but did not carry a
/// usable type. That is still not license to use the unrelated spawn fallback.
fn starts_agent_type(stage_dirs: &[PathBuf], agent_id: &str) -> Option<Option<String>> {
    let mut found = false;
    let mut types = Vec::new();
    for stage_dir in stage_dirs {
        for entry in json_lines(&stage_dir.join("starts.jsonl")) {
            if entry.get("agent_id").and_then(Value::as_str) == Some(agent_id) {
                found = true;
                let Some(agent_type) = entry
                    .get("agent_type")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                else {
                    return Some(None);
                };
                types.push(agent_type.to_owned());
            }
        }
    }
    found.then(|| unanimous(types))
}

fn unanimous(values: impl IntoIterator<Item = String>) -> Option<String> {
    let mut agreed: Option<String> = None;
    for value in values {
        if agreed.as_ref().is_some_and(|known| known != &value) {
            return None;
        }
        agreed.get_or_insert(value);
    }
    agreed
}

/// A spawn ledger predates the agent id, so it is not a per-agent lookup.
/// It is only safe fallback evidence when every usable row across the stages
/// we must scan agrees on one type; any disagreement means the owner cannot
/// be inferred and must remain unknown.
fn spawns_agent_type(stage_dirs: &[PathBuf]) -> Option<String> {
    let mut agreed: Option<String> = None;
    for stage_dir in stage_dirs {
        for entry in json_lines(&stage_dir.join("spawns.jsonl")) {
            let Some(agent_type) = entry.get("agent_type").and_then(Value::as_str) else {
                continue;
            };
            if agreed.as_deref().is_some_and(|known| known != agent_type) {
                return None;
            }
            agreed.get_or_insert_with(|| agent_type.to_string());
        }
    }
    agreed
}

/// Parse independently so a hook appending a partial line cannot hide the
/// valid ledger records before it. Missing or unreadable ledgers simply have
/// no usable rows.
fn json_lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .ok()
        .map(|content| {
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
