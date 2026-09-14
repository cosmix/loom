//! Exact lifecycle evidence lookup for transcript classification.

use std::fs;
use std::path::{Path, PathBuf};

use crate::subagent_lifecycle::{self, LifecycleIndex, WorkerOutcome};

use super::super::forward_jobs::FORWARDER_TYPE;
use super::super::ledger::StartedAgentTypeIndex;
use super::{DoneEvidence, SubagentState, SubagentSummary};

pub(in crate::commands::subagents) struct Context {
    stage_id: String,
    loom_session_id: String,
    index: Option<LifecycleIndex>,
    replay_error: Option<String>,
    starts: StartedAgentTypeIndex,
}

pub(super) struct Evidence {
    pub(super) agent_type: Option<String>,
    outcome: WorkerOutcome,
}

struct TranscriptIdentity {
    parent_session_id: String,
    path: PathBuf,
}

pub(in crate::commands::subagents) fn load_active(work_dir: Option<&Path>) -> Option<Context> {
    let stage_id = std::env::var("LOOM_STAGE_ID").ok()?;
    let loom_session_id = std::env::var("LOOM_SESSION_ID").ok()?;
    Some(Context::load(work_dir, stage_id, loom_session_id))
}

impl Context {
    pub(in crate::commands::subagents) fn load(
        work_dir: Option<&Path>,
        stage_id: String,
        loom_session_id: String,
    ) -> Self {
        let replayed = work_dir.map(subagent_lifecycle::replay);
        let (index, replay_error) = match replayed {
            Some(Ok(index)) => (Some(index), None),
            Some(Err(error)) => (None, Some(error.to_string())),
            None => (None, Some("loom work directory is unavailable".into())),
        };
        Self {
            stage_id,
            loom_session_id,
            index,
            replay_error,
            starts: StartedAgentTypeIndex::load(work_dir),
        }
    }

    pub(super) fn evidence(&self, transcript: &Path, agent_id: &str) -> Evidence {
        let identity = match transcript_identity(transcript, agent_id) {
            Ok(identity) => identity,
            Err(reason) => return Evidence::unknown(reason),
        };
        let agent_type = self
            .starts
            .resolve_exact(
                &self.stage_id,
                &identity.parent_session_id,
                &self.loom_session_id,
                agent_id,
            )
            .map(|metadata| metadata.agent_type);
        let Some(index) = &self.index else {
            let reason = self
                .replay_error
                .clone()
                .unwrap_or_else(|| "lifecycle replay is unavailable".into());
            return Evidence {
                agent_type,
                outcome: WorkerOutcome::Unknown(reason),
            };
        };
        let outcome = if agent_type.as_deref() == Some(FORWARDER_TYPE) {
            index.forwarded_outcome(
                &self.stage_id,
                &self.loom_session_id,
                &identity.parent_session_id,
                agent_id,
            )
        } else {
            index.claude_outcome(
                &self.stage_id,
                &self.loom_session_id,
                &identity.parent_session_id,
                agent_id,
                &identity.path,
            )
        };
        Evidence {
            agent_type,
            outcome,
        }
    }
}

impl Evidence {
    fn unknown(reason: String) -> Self {
        Self {
            agent_type: None,
            outcome: WorkerOutcome::Unknown(reason),
        }
    }

    pub(super) fn apply(self, summary: &mut SubagentSummary) {
        let forwarder = self.agent_type.as_deref() == Some(FORWARDER_TYPE);
        match self.outcome {
            WorkerOutcome::Succeeded => {
                summary.state = SubagentState::Done;
                summary.done_evidence = Some(DoneEvidence::Lifecycle);
                summary.display_state = None;
            }
            WorkerOutcome::Failed(reason) => {
                summary.state = SubagentState::Failed;
                summary.done_evidence = None;
                summary.terminal_reason = Some(reason);
                summary.display_state = None;
            }
            WorkerOutcome::Cancelled(reason) => {
                summary.state = SubagentState::Cancelled;
                summary.done_evidence = None;
                summary.terminal_reason = Some(reason);
                summary.display_state = None;
            }
            WorkerOutcome::Active if forwarder => {
                summary.state = SubagentState::ForwardWait;
                summary.done_evidence = None;
                summary.final_report = None;
                summary.display_state = None;
            }
            WorkerOutcome::Unknown(reason) if forwarder => {
                summary.state = SubagentState::ForwardUnknown;
                summary.done_evidence = None;
                summary.terminal_reason = Some(reason);
                summary.final_report = None;
                summary.display_state = None;
            }
            WorkerOutcome::Active | WorkerOutcome::Unknown(_) | WorkerOutcome::Stalled(_) => {}
        }
    }
}

fn transcript_identity(transcript: &Path, agent_id: &str) -> Result<TranscriptIdentity, String> {
    let path =
        fs::canonicalize(transcript).map_err(|error| format!("normalizing transcript: {error}"))?;
    let expected = format!("agent-{agent_id}.jsonl");
    if path.file_name().and_then(|value| value.to_str()) != Some(expected.as_str()) {
        return Err("transcript filename does not match agent id".into());
    }
    let subagents = path
        .parent()
        .ok_or_else(|| "transcript has no parent".to_string())?;
    if subagents.file_name().and_then(|value| value.to_str()) != Some("subagents") {
        return Err("transcript is not under a subagents directory".into());
    }
    let parent = subagents
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "transcript has no Claude parent UUID directory".to_string())?;
    Ok(TranscriptIdentity {
        parent_session_id: parent.to_owned(),
        path,
    })
}
