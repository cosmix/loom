//! Whether one ticketed line may be relayed for this Bash call.
//!
//! Beyond the kind checks, a control ticket the lead relays must be tied to
//! the lead's own call: `loom-relay-guard.sh` writes an empty marker
//! `<scratch_dir>/.pre-<tool_use_id>` at PreToolUse, and the ticket must have
//! been written at or after it. A ticket a subagent planted earlier in the
//! shared scratch directory is older than the marker and is refused.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::{label, sweep, ticket_check, Request};
use crate::relay::{AgentRole, RelayLine, RequestKind};

/// Longest `tool_use_id` that gets a marker; mirrors `loom-relay-guard.sh`.
const MAX_TOOL_USE_ID_LEN: usize = 128;

/// `<scratch_dir>/.pre-<tool_use_id>`, or `None` for an id the guard would not
/// have written a marker for: 1 to 128 ASCII alphanumerics, `_` or `-`.
pub(super) fn marker_path(scratch_dir: &Path, tool_use_id: &str) -> Option<PathBuf> {
    let valid = (1..=MAX_TOOL_USE_ID_LEN).contains(&tool_use_id.len())
        && tool_use_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    valid.then(|| scratch_dir.join(format!(".pre-{tool_use_id}")))
}

/// Remove this call's marker once its relay ran. Every error is ignored: the
/// guard's age sweep removes whatever is left behind.
pub(super) fn discard_call_marker(scratch_dir: &Path, tool_use_id: Option<&str>) {
    if let Some(marker) = tool_use_id.and_then(|id| marker_path(scratch_dir, id)) {
        let _ = std::fs::remove_file(marker);
    }
}

/// Require that a control ticket was written inside this call's window: at or
/// after the call marker the guard wrote, and not after `now`. The ticket's
/// mtime comes from the open descriptor its bytes were read through; its
/// `created_at` field is written by the same untrusted session and proves
/// nothing.
///
/// Known limit: a subagent that rewrites its ticket continuously while the
/// lead's call runs can still land inside the window. Revisit if tickets gain
/// a writer identity.
pub(super) fn require_in_call_window(
    scratch_dir: &Path,
    tool_use_id: Option<&str>,
    ticket_modified: SystemTime,
    uid: u32,
    now: SystemTime,
) -> Result<(), String> {
    let marker = tool_use_id
        .and_then(|id| marker_path(scratch_dir, id))
        .ok_or(
            "this call carries no usable tool_use_id, so its control request cannot be tied to \
             it. Rerun the loom command alone, as its own Bash call",
        )?;
    let metadata = match marker.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(
                "no call marker from loom-relay-guard.sh exists for this call, so the \
                        ticket cannot be shown to come from it. Rerun the loom command alone, \
                        as its own Bash call"
                    .to_string(),
            );
        }
        Err(_) => return Err(not_a_plain_marker()),
    };
    if !metadata.file_type().is_file() || metadata.uid() != uid {
        return Err(not_a_plain_marker());
    }
    let marked_at = metadata.modified().map_err(|_| not_a_plain_marker())?;
    if ticket_modified < marked_at {
        return Err(
            "the ticket was written before this call started, so this call's own \
                    command did not write it"
                .to_string(),
        );
    }
    if ticket_modified > now {
        return Err("the ticket's modification time lies in the future".to_string());
    }
    Ok(())
}

fn not_a_plain_marker() -> String {
    "the call marker for this call is not a plain file this user owns".to_string()
}

/// A payload carrying a non-empty `agent_type` came from a subagent.
pub(super) fn agent_role(payload: &Value) -> AgentRole {
    match payload.get("agent_type").and_then(Value::as_str) {
        Some(agent_type) if !agent_type.is_empty() => AgentRole::Subagent,
        _ => AgentRole::Main,
    }
}

impl Request<'_> {
    /// Admit, verify, write and consume one ticketed line; one reply line.
    pub(super) fn relay_one(&self, line: &RelayLine) -> String {
        let label = label(line);
        if let Err(reason) = self.admit(line.kind) {
            return self.refuse(line, &label, &reason);
        }
        let (ticket, modified) =
            match ticket_check::read_verified(&self.env.scratch_dir, line, self.env.uid) {
                Ok(read) => read,
                Err(error) => return format!("LOOM relay: refused {label}: {error:#}"),
            };
        if line.kind.is_control() && self.agent == AgentRole::Main {
            if let Err(reason) = require_in_call_window(
                &self.env.scratch_dir,
                self.tool_use_id.as_deref(),
                modified,
                self.env.uid,
                SystemTime::now(),
            ) {
                return format!("LOOM relay: refused {label}: {reason}");
            }
        }
        match self.record(ticket, sweep::Source::Line) {
            sweep::Recorded::Reply(reply) | sweep::Recorded::Full(reply) => reply,
        }
    }

    /// The reply for a line `admit` refused. A control ticket a subagent
    /// printed is deleted once it verifies against its line, so no later lead
    /// call can replay it; any other refusal keeps the ticket.
    fn refuse(&self, line: &RelayLine, label: &str, reason: &str) -> String {
        if self.agent != AgentRole::Subagent || !line.kind.is_control() {
            return format!("LOOM relay: refused {label}: {reason}");
        }
        if let Err(error) = ticket_check::read_verified(&self.env.scratch_dir, line, self.env.uid) {
            return format!("LOOM relay: refused {label}: {error:#}");
        }
        let note =
            ticket_check::consume(&ticket_check::ticket_file(&self.env.scratch_dir, &line.id));
        let outcome = if note.is_empty() {
            " Its ticket was deleted.".to_string()
        } else {
            note
        };
        format!("LOOM relay: refused {label}: {reason}.{outcome}")
    }

    /// Control kinds never come from a subagent, and a kind the Bash command
    /// did not run the writer for is not relayed.
    fn admit(&self, kind: RequestKind) -> Result<(), String> {
        if self.agent == AgentRole::Subagent && kind.is_control() {
            let control_kinds = RequestKind::all()
                .into_iter()
                .filter(|control_kind| control_kind.is_control())
                .map(|control_kind| control_kind.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "control requests ({control_kinds}) are never relayed from a subagent. Ask \
                 the main agent to run the command"
            ));
        }
        if !self.allowed.contains(&kind) {
            return Err(format!(
                "this Bash call did not run the loom command that writes {kind} requests, so \
                 its line was not relayed. Run that loom command on its own, with its stdout \
                 unfiltered"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests_admission.rs"]
mod tests;
