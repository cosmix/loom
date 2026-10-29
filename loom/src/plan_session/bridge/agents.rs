use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::plan_session::spool::Tail;
use crate::provider_data::resolve::agent_id_from_path;

const MAX_META_BYTES: usize = 64 * 1024;

pub(super) struct AgentFeed {
    pub(super) tail: Tail,
    pub(super) agent_id: String,
    pub(super) spawned_by: Option<String>,
    pub(super) teammate: Option<String>,
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
            first_seen: Instant::now(),
            classified: false,
        }
    }

    pub(super) fn classify(&mut self, dir: &Path, grace: Duration) -> bool {
        if self.classified {
            return true;
        }
        let name = PathBuf::from(format!("agent-{}.meta.json", self.agent_id));
        let meta: Value = match read_bounded(dir, &name, MAX_META_BYTES) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(meta) => meta,
                // An empty or half-written file is still being published.
                Err(_) if self.first_seen.elapsed() < grace => return false,
                Err(_) => Value::Null,
            },
            // Leave the tail unread while Claude Code finishes publishing metadata.
            Err(error) if is_not_found(&error) && self.first_seen.elapsed() < grace => {
                return false;
            }
            Err(_) => Value::Null,
        };
        self.spawned_by = meta
            .get("toolUseId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.teammate = (meta.get("taskKind").and_then(Value::as_str)
            == Some("in_process_teammate"))
        .then(|| {
            meta.get("name")
                .and_then(Value::as_str)
                .unwrap_or(&self.agent_id)
                .to_owned()
        });
        self.classified = true;
        true
    }
}
