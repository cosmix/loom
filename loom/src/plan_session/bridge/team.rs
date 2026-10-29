//! Visible progress of in-process teammates, without publishing their chat messages.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::plan_session::model::TokenCounts;
use crate::plan_session::transcript::Mapper;
use crate::tokens::tail::MAX_LINE_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeammateState {
    Working,
    Idle,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TeammateStatus {
    pub agent_id: String,
    pub name: String,
    /// Repository-relative documents, in first-seen order.
    pub documents: Vec<String>,
    pub current: Option<String>,
    pub state: TeammateState,
    pub tokens: TokenCounts,
}

struct Teammate {
    status: TeammateStatus,
    usage: Mapper,
}

pub struct TeamTracker {
    project_root: PathBuf,
    teammates: HashMap<String, Teammate>,
}

impl TeamTracker {
    pub fn new(project_root: PathBuf) -> TeamTracker {
        Self {
            project_root: normalize(&project_root).unwrap_or(project_root),
            teammates: HashMap::new(),
        }
    }

    /// Ingest complete transcript lines; report only changes to visible status.
    pub fn ingest(&mut self, agent_id: &str, name: &str, lines: &[String]) -> bool {
        let previous = self.teammates.get(agent_id).map(|mate| mate.status.clone());
        let root = &self.project_root;
        let mate = self
            .teammates
            .entry(agent_id.to_owned())
            .or_insert_with(|| Teammate::new(agent_id, name, root));
        if !name.is_empty() {
            mate.status.name = name.to_owned();
        }
        for line in lines {
            mate.ingest(root, line);
        }
        mate.status.current = match mate.status.state {
            TeammateState::Working => mate.status.documents.last().cloned(),
            TeammateState::Idle => None,
        };
        previous.as_ref() != Some(&mate.status)
    }

    /// Sorted by name, then agent id.
    pub fn statuses(&self) -> Vec<TeammateStatus> {
        let mut statuses: Vec<_> = self
            .teammates
            .values()
            .map(|mate| mate.status.clone())
            .collect();
        statuses.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });
        statuses
    }
}

impl Teammate {
    fn new(agent_id: &str, name: &str, root: &Path) -> Self {
        let mut usage = Mapper::new(root.to_path_buf());
        // One permanently open turn reuses the mapper's usage parser and message-id dedup.
        usage.begin_turn(chrono::DateTime::UNIX_EPOCH);
        Self {
            status: TeammateStatus {
                agent_id: agent_id.to_owned(),
                name: name.to_owned(),
                documents: Vec::new(),
                current: None,
                state: TeammateState::Working,
                tokens: TokenCounts::default(),
            },
            usage,
        }
    }

    fn ingest(&mut self, root: &Path, line: &str) {
        if line.len() > MAX_LINE_BYTES {
            return;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(kind) = record.get("type").and_then(Value::as_str) else {
            return;
        };
        self.status.state = TeammateState::Working;
        if kind != "assistant" {
            return;
        }
        self.usage.ingest_tokens_only(line);
        if let Some(turn) = self.usage.turns().first() {
            self.status.tokens = turn.tokens.clone();
        }
        if record
            .pointer("/message/stop_reason")
            .and_then(Value::as_str)
            == Some("end_turn")
        {
            self.status.state = TeammateState::Idle;
        }
        let blocks = record.pointer("/message/content").and_then(Value::as_array);
        for block in blocks.into_iter().flatten() {
            if let Some(path) = document(root, block) {
                if !self.status.documents.contains(&path) {
                    self.status.documents.push(path);
                }
            }
        }
    }
}

fn document(root: &Path, block: &Value) -> Option<String> {
    if block.get("type")?.as_str()? != "tool_use"
        || !matches!(block.get("name")?.as_str()?, "Write" | "Edit" | "MultiEdit")
    {
        return None;
    }
    let path = Path::new(block.pointer("/input/file_path")?.as_str()?);
    let absolute = normalize(&root.join(path))?;
    let relative = absolute.strip_prefix(root).ok()?;
    if !relative.starts_with("doc") || relative.components().count() < 2 {
        return None;
    }
    Some(relative.to_str()?.to_owned())
}

fn normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized)
}

#[cfg(test)]
#[path = "team_tests.rs"]
mod tests;
