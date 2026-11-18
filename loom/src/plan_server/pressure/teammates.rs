//! In-process teammates' token usage for a pressure step.
//!
//! Claude Code (confirmed on 2.1.312, and in transcripts written by 2.1.295) writes an
//! in-process teammate's transcript where it writes a subagent's:
//! `<transcript dir>/<session id>/subagents/agent-<id>.jsonl`, beside `agent-<id>.meta.json`
//! whose `taskKind` is `in_process_teammate` (a subagent's meta has none, and names the spawning
//! `toolUseId` instead). Each teammate transcript found there is registered with the bridge's
//! `add_token_transcript`, so its usage reaches the open turn's `tokens` through the bridge's
//! own mapper and adds no chat item. The mapper keys usage by message id, so a file the bridge
//! also tails as a subagent is never counted twice.
//!
//! Revisit if Claude Code moves teammate transcripts (a `transcriptSubdir`, today set only for
//! workflow agents) or stops writing `taskKind` into the meta file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::driver::PressureSessions;
use crate::fs::safe_read::read_bounded;
use crate::plan_session::bridge::Transcripts;

/// A meta file holds a few ids and settings; the bound stops a planted giant file.
const MAX_META_BYTES: usize = 64 * 1024;
/// The meta value that marks an in-process teammate.
const TEAMMATE_TASK: &str = "in_process_teammate";

/// The teammate transcripts of one Claude step already registered with the bridge.
#[derive(Default)]
pub(super) struct Teammates {
    registered: HashSet<PathBuf>,
}

impl Teammates {
    /// Register every teammate transcript of session `id` that appeared since the last call.
    pub(super) fn register_new(&mut self, sessions: &dyn PressureSessions, id: &str) {
        let Some(transcripts) = sessions.transcripts(id) else {
            return;
        };
        for path in teammate_transcripts(&transcripts) {
            if self.registered.contains(&path) {
                continue;
            }
            match sessions.add_token_transcript(id, path.clone()) {
                Ok(()) => {
                    self.registered.insert(path);
                }
                Err(error) => {
                    tracing::debug!(session = %id, %error, "teammate transcript not registered");
                }
            }
        }
    }
}

/// The session's agent transcripts whose meta file marks an in-process teammate.
pub(super) fn teammate_transcripts(transcripts: &Transcripts) -> Vec<PathBuf> {
    let agents = transcripts.subagents.iter();
    let teammates = agents.filter(|(agent, path)| is_teammate(path, agent));
    teammates.map(|(_, path)| path.clone()).collect()
}

/// `agent-<id>.meta.json` beside the transcript, read bounded and without following links.
fn is_teammate(transcript: &Path, agent: &str) -> bool {
    let Some(dir) = transcript.parent() else {
        return false;
    };
    let meta = PathBuf::from(format!("agent-{agent}.meta.json"));
    let Ok(bytes) = read_bounded(dir, &meta, MAX_META_BYTES) else {
        return false;
    };
    let meta: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    meta.get("taskKind").and_then(serde_json::Value::as_str) == Some(TEAMMATE_TASK)
}
