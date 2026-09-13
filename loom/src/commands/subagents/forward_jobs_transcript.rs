//! Minimal, read-only extraction of forwarding Bash calls from a transcript.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::forward_receipt::marker::MarkerChannel;
use crate::models::forward_receipt::transcript::{
    self as transcript_reader, ForwardingResult, Transcript,
};
use crate::models::forward_receipt::{ForwardIdentity, ForwardState};

use super::{classify, forward_jobs::ForwardIndex};

#[derive(Debug, Clone)]
pub(super) struct ForwardToolUse {
    pub(super) id: String,
}

pub(super) struct ForwardTranscript {
    uses: Vec<ForwardToolUse>,
    results: HashMap<String, ForwardingResult>,
    parent_session_id: Option<String>,
    pub(super) done: bool,
    pub(super) unreadable: bool,
}

pub(super) fn read(path: &Path) -> ForwardTranscript {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ForwardTranscript::empty(),
        Err(_) => ForwardTranscript::unreadable(),
        Ok(_) => transcript_reader::read(path)
            .map(ForwardTranscript::from_shared)
            .unwrap_or_else(|_| ForwardTranscript::unreadable()),
    }
}

impl ForwardTranscript {
    fn empty() -> Self {
        Self {
            uses: Vec::new(),
            results: HashMap::new(),
            parent_session_id: None,
            done: false,
            unreadable: false,
        }
    }

    fn unreadable() -> Self {
        Self {
            unreadable: true,
            ..Self::empty()
        }
    }

    fn from_shared(transcript: Transcript) -> Self {
        let done = transcript
            .entries()
            .last()
            .is_some_and(classify::is_done_entry);
        let parent_session_id = Some(transcript.identity.parent_session_id.clone());
        let mut uses = Vec::new();
        let mut results = HashMap::new();
        for invocation in transcript.invocations {
            if let Some(result) = invocation.result {
                results.insert(invocation.id.clone(), result);
            }
            uses.push(ForwardToolUse { id: invocation.id });
        }
        Self {
            uses,
            results,
            parent_session_id,
            done,
            unreadable: false,
        }
    }

    pub(super) fn has_forwarding_use(&self) -> bool {
        !self.uses.is_empty()
    }

    pub(super) fn forwarding_uses(&self) -> impl Iterator<Item = &ForwardToolUse> {
        self.uses.iter()
    }

    pub(super) fn parent_session_id(&self) -> Option<&str> {
        self.parent_session_id.as_deref()
    }

    pub(super) fn has_identity(
        &self,
        index: &ForwardIndex,
        agent_id: &str,
        tool_use_id: &str,
    ) -> bool {
        let Some(parent) = self.parent_session_id.as_deref() else {
            return false;
        };
        let Some(loom_session) = index.loom_session_id.as_deref() else {
            return false;
        };
        ForwardIdentity::new(parent, agent_id, tool_use_id, &index.stage_id, loom_session).is_ok()
    }

    pub(super) fn marker_for(
        &self,
        tool: &ForwardToolUse,
        roots: &[PathBuf],
    ) -> Option<ForwardState> {
        let result = self.results.get(&tool.id)?;
        let parent = self.parent_session_id.as_deref()?;
        marker_state(transcript_reader::evidence_channel(result, roots, parent))
    }
}

fn marker_state(channel: MarkerChannel) -> Option<ForwardState> {
    match channel {
        MarkerChannel::Streaming(_) | MarkerChannel::Started(_) => Some(ForwardState::Running),
        MarkerChannel::Finished(_, end) => Some(end.outcome),
        MarkerChannel::Absent | MarkerChannel::Invalid(_) => None,
    }
}
