use std::path::Path;

use anyhow::{ensure, Result};

use super::super::is_safe_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptIdentity {
    pub parent_session_id: String,
    pub agent_id: String,
}

impl TranscriptIdentity {
    pub fn from_path(path: &Path) -> Result<Self> {
        let agent_id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(|value| value.strip_prefix("agent-"));
        let subagents = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str());
        let parent_session_id = path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|value| value.to_str());
        ensure!(
            subagents == Some("subagents"),
            "not a subagent transcript path"
        );
        let (Some(agent_id), Some(parent_session_id)) = (agent_id, parent_session_id) else {
            anyhow::bail!("incomplete subagent transcript identity");
        };
        ensure!(
            is_safe_id(agent_id) && is_safe_id(parent_session_id),
            "unsafe transcript identity"
        );
        Ok(Self {
            parent_session_id: parent_session_id.to_owned(),
            agent_id: agent_id.to_owned(),
        })
    }
}
