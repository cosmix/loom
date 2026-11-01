//! Recovering a handoff from the handoff DOCUMENT rather than the stage file.
//!
//! A worktree session runs sandboxed: it may write `.loom/work/handoffs` but not
//! `.loom/work/stages`. So when an agent hits its context ceiling and runs
//! `loom handoff --trigger ceiling`, the document lands and the
//! `Executing -> NeedsHandoff` transition does not. Everything downstream of
//! the status — [`super::budget_latch::needs_handoff_event`], the takedown,
//! the re-queue — is level-triggered on that status, so none of it ever armed
//! and the stage sat `Executing` behind an agent that had already stopped.
//!
//! The document is the signal the sandbox does allow, so the daemon reads it
//! directly. A document is a takedown request only when it carries
//! [`HandoffOrigin::AgentCeiling`]: precompact, session_end and manual
//! documents record context for a session that is still working, and the
//! daemon's own advisory snapshots carry their own origins. Acting on those
//! would kill a healthy agent every time it compacted.
//!
//! # Cost
//!
//! This runs once per poll tick (5 s). Successfully inspected artifacts are
//! immutable and cached by filename. Failed or incomplete reads retry with
//! bounded backoff; a failed read cannot become a permanent negative result.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::fs::safe_read::read_bounded;
use crate::handoff::HandoffOrigin;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

use super::events::MonitorEvent;

/// How much of a document's head is read to find its frontmatter.
///
/// The identity fields are serialized first and the body that follows the
/// frontmatter can run to tens of kilobytes, so reading the whole file to
/// learn two strings is waste. A document whose frontmatter does not close
/// within this budget is not evidence of anything and is skipped.
const FRONTMATTER_SCAN_BYTES: usize = 8 * 1024;
const MAX_HANDOFF_BYTES: usize = 1024 * 1024;
const MAX_RETRY_BACKOFF_EXPONENT: u32 = 4;

/// The takedown a handoff document asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TakedownRequest {
    session_id: String,
    stage_id: String,
}

/// The frontmatter fields this watch reads. Deliberately a subset of
/// `HandoffV2`: a document missing unrelated fields still names its session
/// perfectly well, and failing to parse it would lose a real request.
#[derive(Debug, Deserialize)]
struct HandoffFrontmatter {
    session_id: String,
    stage_id: String,
    #[serde(default)]
    origin: Option<HandoffOrigin>,
}

/// A successfully inspected immutable document, or a read to retry.
enum DocumentState {
    NonRequest,
    Request,
    Retry { failures: u32, next_poll: u64 },
}

/// Per-filename memory of handoff documents and their takedown requests.
#[derive(Default)]
pub(super) struct HandoffWatch {
    examined: HashMap<String, DocumentState>,
    requests: HashSet<(String, String)>,
    poll: u64,
    #[cfg(test)]
    directory_reads: u64,
}

impl HandoffWatch {
    /// The `SessionNeedsHandoff` a handoff document asks for, if one does.
    ///
    /// Only for a stage still `Executing` and still naming this session: a
    /// stage already in `NeedsHandoff` belongs to the status-triggered path,
    /// and re-emitting there would duplicate its work for nothing.
    ///
    /// Re-emitting on later ticks is safe and deliberate. `on_needs_handoff`
    /// is guarded by `event_targets_current_session`, and leaving the stage in
    /// `NeedsHandoff` after a failed takedown is exactly how the existing
    /// retry works — a suppression latch here would swallow that retry.
    pub(super) fn needs_handoff_from_document(
        &self,
        session: &Session,
        stages: &[Stage],
    ) -> Option<MonitorEvent> {
        let stage_id = session.stage_id.as_deref()?;
        if !stage_is_executing_for(stages, stage_id, &session.id) {
            return None;
        }

        let requested = self
            .requests
            .contains(&(stage_id.to_string(), session.id.clone()));

        requested.then(|| MonitorEvent::SessionNeedsHandoff {
            session_id: session.id.clone(),
            stage_id: stage_id.to_string(),
        })
    }

    /// Inspect new documents and retry uncertain reads once their backoff ends.
    /// The caller invokes this once per poll, before inspecting any session.
    pub(super) fn refresh(&mut self, work_dir: &Path) {
        self.poll = self.poll.saturating_add(1);
        let handoffs_dir = work_dir.join("handoffs");
        #[cfg(test)]
        {
            self.directory_reads += 1;
        }
        let entries = match std::fs::read_dir(&handoffs_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(path = %handoffs_dir.display(), %error, "Cannot list handoffs");
                return;
            }
        };
        let root = match std::fs::canonicalize(work_dir) {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!(path = %work_dir.display(), %error, "Cannot resolve work directory for handoff read");
                return;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(%error, "Cannot list a handoff entry");
                    continue;
                }
            };
            let Some(filename) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            self.inspect_filename(&root, filename);
        }
    }

    fn inspect_filename(&mut self, root: &Path, filename: String) {
        if !filename.ends_with(".md") {
            return;
        }
        let failures = match self.examined.get(&filename) {
            Some(DocumentState::NonRequest | DocumentState::Request) => return,
            Some(DocumentState::Retry { next_poll, .. }) if *next_poll > self.poll => return,
            Some(DocumentState::Retry { failures, .. }) => *failures,
            None => 0,
        };
        match takedown_request(root, &filename) {
            Ok(Some(request)) => {
                self.requests.insert((request.stage_id, request.session_id));
                self.examined.insert(filename, DocumentState::Request);
            }
            Ok(None) => {
                self.examined.insert(filename, DocumentState::NonRequest);
            }
            Err(error) => {
                let failures = failures.saturating_add(1);
                let delay = 1_u64 << (failures - 1).min(MAX_RETRY_BACKOFF_EXPONENT);
                self.examined.insert(
                    filename.clone(),
                    DocumentState::Retry {
                        failures,
                        next_poll: self.poll.saturating_add(delay),
                    },
                );
                tracing::warn!(%filename, %error, delay_polls = delay, "Cannot inspect handoff; will retry");
            }
        }
    }

    #[cfg(test)]
    pub(super) fn directory_read_count(&self) -> u64 {
        self.directory_reads
    }
}

/// Whether `stages` says this exact session is the one executing `stage_id`.
fn stage_is_executing_for(stages: &[Stage], stage_id: &str, session_id: &str) -> bool {
    stages.iter().any(|stage| {
        stage.id == stage_id
            && stage.status == StageStatus::Executing
            && stage.session.as_deref() == Some(session_id)
    })
}

/// The takedown request a fully inspected document carries, if any.
fn takedown_request(root: &Path, filename: &str) -> Result<Option<TakedownRequest>> {
    let Some(frontmatter) = frontmatter_head(root, filename)? else {
        return Ok(None);
    };
    let Ok(frontmatter) = crate::yaml::from_str::<HandoffFrontmatter>(&frontmatter) else {
        return Ok(None);
    };
    if frontmatter.origin != Some(HandoffOrigin::AgentCeiling) {
        return Ok(None);
    }
    Ok(Some(TakedownRequest {
        session_id: frontmatter.session_id,
        stage_id: frontmatter.stage_id,
    }))
}

/// The YAML frontmatter at the head of a handoff document.
///
/// Safely bounds the entire artifact before inspecting its head. A partial
/// frontmatter or oversized document remains uncertain and is retried.
fn frontmatter_head(root: &Path, filename: &str) -> Result<Option<String>> {
    let relative = Path::new("handoffs").join(filename);
    let document = read_bounded(root, &relative, MAX_HANDOFF_BYTES)
        .with_context(|| format!("Cannot read handoff: {filename}"))?;
    if document.is_empty() {
        bail!("Handoff document is empty");
    }
    let head = String::from_utf8_lossy(&document[..document.len().min(FRONTMATTER_SCAN_BYTES + 1)]);
    let trimmed = head.trim_start();
    let Some(body) = trimmed.strip_prefix("---\n") else {
        if trimmed.starts_with("---") {
            bail!("Handoff frontmatter is incomplete");
        }
        return Ok(None);
    };
    let Some(end) = body
        .find("\n---\n")
        .or_else(|| body.strip_suffix("\n---").map(|s| s.len()))
    else {
        bail!("Handoff frontmatter is incomplete or exceeds {FRONTMATTER_SCAN_BYTES} bytes");
    };
    Ok(Some(body[..end].to_string()))
}

#[cfg(test)]
mod tests;
