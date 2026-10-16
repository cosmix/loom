//! Reads the optional hook-side ledgers that add spawn metadata the Claude
//! transcript format does not carry. Every lookup is deliberately best-effort:
//! this read-only command remains useful before hooks have created a ledger.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MAX_LEDGER_BYTES: u64 = 4 * 1024 * 1024;

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
    /// Fully scoped lifecycle joins. Rows missing either scope never enter it.
    exact: HashMap<(String, String, String, String), MetadataAgreement>,
    /// A scoped start is addressable only by Claude's parent transcript id
    /// and the agent id together. `Unknown` covers conflicting nonempty rows;
    /// empty types are ignored for agreement but still suppress legacy joins.
    scoped: HashMap<(String, String), MetadataAgreement>,
    /// Any scoped row (including the obsolete ambiguous `session_id` schema)
    /// makes an unscoped legacy row for this agent id unsafe to use.
    scoped_agents: HashSet<String>,
    /// Pre-session-schema starts can be used only when all rows for the id
    /// agree and there are no scoped rows for that id.
    legacy: HashMap<String, MetadataAgreement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartedAgentMetadata {
    pub(crate) agent_type: String,
    pub(crate) stage_id: Option<String>,
    pub(crate) loom_session_id: Option<String>,
    pub(crate) started_at: Option<String>,
}

#[derive(Clone)]
enum MetadataAgreement {
    Known(StartedAgentMetadata),
    Unknown,
}

impl MetadataAgreement {
    fn value(&self) -> Option<StartedAgentMetadata> {
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
            let directory_stage = stage_dir.file_name().and_then(|value| value.to_str());
            for entry in json_lines(&stage_dir.join("starts.jsonl")) {
                index.record(&entry, directory_stage);
            }
        }
        index
    }

    #[cfg(test)]
    pub(crate) fn get(&self, agent_id: &str, parent_session_id: &str) -> Option<String> {
        let key = (parent_session_id.to_owned(), agent_id.to_owned());
        if let Some(agreement) = self.scoped.get(&key) {
            return agreement.value().map(|metadata| metadata.agent_type);
        }
        if self.scoped_agents.contains(agent_id) {
            return None;
        }
        self.legacy
            .get(agent_id)
            .and_then(MetadataAgreement::value)
            .map(|metadata| metadata.agent_type)
    }

    pub(crate) fn get_metadata(
        &self,
        agent_id: &str,
        parent_session_id: &str,
    ) -> Option<StartedAgentMetadata> {
        let key = (parent_session_id.to_owned(), agent_id.to_owned());
        self.scoped.get(&key).and_then(MetadataAgreement::value)
    }

    /// Resolve only a fully scoped start row. Legacy and obsolete
    /// `session_id` rows are deliberately absent from this index.
    pub(crate) fn resolve_exact(
        &self,
        stage_id: &str,
        parent_session_id: &str,
        loom_session_id: &str,
        agent_id: &str,
    ) -> Option<StartedAgentMetadata> {
        let key = (
            stage_id.to_owned(),
            parent_session_id.to_owned(),
            loom_session_id.to_owned(),
            agent_id.to_owned(),
        );
        self.exact.get(&key).and_then(MetadataAgreement::value)
    }

    fn record(&mut self, entry: &Value, directory_stage: Option<&str>) {
        let Some(agent_id) = entry.get("agent_id").and_then(Value::as_str) else {
            return;
        };
        let agent_type = entry
            .get("agent_type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());

        match entry.get("parent_session_id") {
            Some(Value::String(parent_session_id)) => {
                self.scoped_agents.insert(agent_id.to_owned());
                if let Some(agent_type) = agent_type {
                    let metadata = start_metadata(entry, agent_type);
                    self.record_exact(parent_session_id, agent_id, directory_stage, &metadata);
                    record_metadata_agreement(
                        self.scoped
                            .entry((parent_session_id.to_owned(), agent_id.to_owned()))
                            .or_insert_with(|| MetadataAgreement::Known(metadata.clone())),
                        metadata,
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
                    let metadata = start_metadata(entry, agent_type);
                    record_metadata_agreement(
                        self.legacy
                            .entry(agent_id.to_owned())
                            .or_insert_with(|| MetadataAgreement::Known(metadata.clone())),
                        metadata,
                    );
                }
            }
        }
    }

    fn record_exact(
        &mut self,
        parent_session_id: &str,
        agent_id: &str,
        directory_stage: Option<&str>,
        metadata: &StartedAgentMetadata,
    ) {
        let (Some(stage_id), Some(loom_session_id)) =
            (&metadata.stage_id, &metadata.loom_session_id)
        else {
            return;
        };
        let key = (
            stage_id.clone(),
            parent_session_id.to_owned(),
            loom_session_id.clone(),
            agent_id.to_owned(),
        );
        let agreement = self
            .exact
            .entry(key)
            .or_insert_with(|| MetadataAgreement::Known(metadata.clone()));
        if directory_stage != Some(stage_id.as_str()) {
            *agreement = MetadataAgreement::Unknown;
            return;
        }
        record_metadata_agreement(agreement, metadata.clone());
    }
}

fn start_metadata(entry: &Value, agent_type: &str) -> StartedAgentMetadata {
    StartedAgentMetadata {
        agent_type: agent_type.to_owned(),
        stage_id: nonempty_string(entry, "stage_id"),
        loom_session_id: nonempty_string(entry, "loom_session_id"),
        started_at: nonempty_string(entry, "ts"),
    }
}

fn nonempty_string(entry: &Value, field: &str) -> Option<String> {
    entry
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn record_metadata_agreement(agreement: &mut MetadataAgreement, incoming: StartedAgentMetadata) {
    let MetadataAgreement::Known(known) = agreement else {
        return;
    };
    if !merge_metadata(known, incoming) {
        *agreement = MetadataAgreement::Unknown;
    }
}

fn merge_metadata(known: &mut StartedAgentMetadata, incoming: StartedAgentMetadata) -> bool {
    if known.agent_type != incoming.agent_type
        || conflicting(&known.stage_id, &incoming.stage_id)
        || conflicting(&known.loom_session_id, &incoming.loom_session_id)
        || conflicting(&known.started_at, &incoming.started_at)
    {
        return false;
    }
    if known.stage_id.is_none() {
        known.stage_id = incoming.stage_id;
    }
    if known.loom_session_id.is_none() {
        known.loom_session_id = incoming.loom_session_id;
    }
    if known.started_at.is_none() {
        known.started_at = incoming.started_at;
    }
    true
}

fn conflicting(left: &Option<String>, right: &Option<String>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left != right)
}

fn stage_directories(work_dir: Option<&Path>) -> Option<Vec<PathBuf>> {
    let work_dir = work_dir?;
    let root = work_dir.join("subagents");
    let root_metadata = fs::symlink_metadata(&root).ok()?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    Some(
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                fs::symlink_metadata(path).is_ok_and(|metadata| {
                    metadata.file_type().is_dir() && !metadata.file_type().is_symlink()
                })
            })
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
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            metadata
        }
        _ => return Vec::new(),
    };
    if metadata.len() > MAX_LEDGER_BYTES {
        return Vec::new();
    }
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let mut content = String::new();
    if file
        .by_ref()
        .take(MAX_LEDGER_BYTES + 1)
        .read_to_string(&mut content)
        .is_err()
    {
        return Vec::new();
    }
    if !content.ends_with('\n') {
        content.truncate(content.rfind('\n').map_or(0, |index| index + 1));
    }
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
