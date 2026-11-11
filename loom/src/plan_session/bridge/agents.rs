//! One subagent transcript's reader, and its `agent-<id>.meta.json` beside it, read once,
//! size-bounded and without following links.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::plan_session::spool::Tail;
use crate::plan_session::transcript::{bounded_id, cap_chars, LINE_CHARS};
use crate::provider_data::resolve::agent_id_from_path;

const MAX_META_BYTES: usize = 64 * 1024;

/// What a subagent's meta file says (probe P1, "Meta file").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentMeta {
    /// `toolUseId`: the call that spawned the agent.
    pub tool_use_id: Option<String>,
    /// `agentType`.
    pub agent_type: Option<String>,
    pub description: Option<String>,
    /// `finished: true`, written once the agent handed back.
    pub finished: bool,
    /// The teammate's name (its id when the meta has none), for `taskKind: in_process_teammate`.
    pub teammate: Option<String>,
}

/// What reading a meta file gave.
enum MetaRead {
    Read(AgentMeta),
    /// Missing or not yet whole: Claude Code may still be publishing it.
    Pending,
    /// Refused (a link, an oversized or unreadable file): it will not become readable.
    Refused,
}

/// The meta file of agent `agent_id` in `subagents_dir`; `None` when it is missing, unreadable
/// or not JSON.
pub fn read_agent_meta(subagents_dir: &Path, agent_id: &str) -> Option<AgentMeta> {
    match read_meta(subagents_dir, agent_id) {
        MetaRead::Read(meta) => Some(meta),
        MetaRead::Pending | MetaRead::Refused => None,
    }
}

fn read_meta(dir: &Path, agent_id: &str) -> MetaRead {
    // The id names one file beside the transcripts, never a path below or above them.
    if agent_id.is_empty() || agent_id.contains('/') {
        return MetaRead::Refused;
    }
    let name = PathBuf::from(format!("agent-{agent_id}.meta.json"));
    match read_bounded(dir, &name, MAX_META_BYTES) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(meta) => MetaRead::Read(meta_of(&meta, agent_id)),
            Err(_) => MetaRead::Pending,
        },
        Err(error) if is_not_found(&error) => MetaRead::Pending,
        Err(_) => MetaRead::Refused,
    }
}

fn meta_of(meta: &Value, agent_id: &str) -> AgentMeta {
    let text = |key: &str| meta.get(key).and_then(Value::as_str);
    let capped = |key: &str| text(key).map(|value| cap_chars(value, LINE_CHARS));
    let teammate = text("taskKind") == Some("in_process_teammate");
    AgentMeta {
        tool_use_id: text("toolUseId").and_then(bounded_id),
        agent_type: capped("agentType"),
        description: capped("description"),
        finished: meta.get("finished").and_then(Value::as_bool) == Some(true),
        teammate: teammate.then(|| text("name").unwrap_or(agent_id).to_owned()),
    }
}

pub(super) struct AgentFeed {
    pub(super) tail: Tail,
    pub(super) agent_id: String,
    pub(super) spawned_by: Option<String>,
    pub(super) teammate: Option<String>,
    /// The meta read at classification, until the next batch carries it.
    pub(super) meta: Option<AgentMeta>,
    first_seen: Instant,
    classified: bool,
}

impl AgentFeed {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            agent_id: agent_id_from_path(&path),
            tail: Tail::new(path),
            spawned_by: None,
            teammate: None,
            meta: None,
            first_seen: Instant::now(),
            classified: false,
        }
    }

    /// Read the meta file once. A missing or half-written one is waited for up to `grace` (the
    /// tail stays unread meanwhile); after that, or when it is refused, the agent is an
    /// ordinary subagent with no meta.
    pub(super) fn classify(&mut self, dir: &Path, grace: Duration) -> bool {
        if self.classified {
            return true;
        }
        let meta = match read_meta(dir, &self.agent_id) {
            MetaRead::Read(meta) => Some(meta),
            MetaRead::Pending if self.first_seen.elapsed() < grace => return false,
            MetaRead::Pending | MetaRead::Refused => None,
        };
        self.spawned_by = meta.as_ref().and_then(|meta| meta.tool_use_id.clone());
        self.teammate = meta.as_ref().and_then(|meta| meta.teammate.clone());
        self.meta = meta;
        self.classified = true;
        true
    }
}
