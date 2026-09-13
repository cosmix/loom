//! `loom hook relay` — the trusted half of `loom-hooks/loom-relay.sh`.
//!
//! A confined session cannot write `.loom/work`. A loom command that has to
//! record something leaves a ticket in its scratch directory and prints one
//! `LOOM_RELAY_V1` line as its last stdout line. This helper runs from the
//! PostToolUse hook, outside the sandbox: it proves the Bash call ran inside
//! the session its environment names, verifies each ticket against its line,
//! and writes the request into the daemon inbox
//! (`doc/plans/PLAN-loom-state-confinement.md` section 7).
//!
//! The tool output is never trusted. A line only selects a ticket, and a
//! ticket only counts when it sits in this session's own scratch directory and
//! matches the line's size and hash. Attribution comes from the environment,
//! never from the ticket. Every refusal is reported; a line with no ticket
//! behind it (a test fixture, an echoed transcript) is dropped silently.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;

use super::target::non_empty_env;
use crate::daemon::caller_is_inside_session;
use crate::fs::inbox::{write_entry, WriteOutcome};
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionStatus;
use crate::relay::{
    scratch_root_from_env, session_dir, validate_session_dir, AgentRole, InboxEntry, RelayLine,
    RequestKind, MAX_PENDING_ENTRIES,
};

mod output;
mod ticket_check;

/// Longest hook payload read from stdin. Claude Code persists a large tool
/// output to a file and inlines only a preview, so a real payload is far
/// smaller; the bound only keeps a pathological one out of memory.
const MAX_STDIN_BYTES: u64 = 8 * 1024 * 1024;

/// Wire version of the inbox entries written here (`relay::InboxEntry`).
const INBOX_ENTRY_VERSION: u32 = 1;

/// Everything the relay reads from its process environment, captured once so
/// the logic below never touches `std::env` and tests need none.
#[derive(Debug, Clone)]
struct RelayEnv {
    /// `LOOM_SESSION_ID`: the session the Bash call claims to run in.
    session_id: String,
    /// `LOOM_STAGE_ID`: the stage every entry is attributed to.
    stage_id: String,
    /// `LOOM_SCRATCH_DIR`: where the CLI left its tickets.
    scratch_dir: PathBuf,
    /// The scratch root loom derives itself; `scratch_dir` must be
    /// `<scratch_root>/<session_id>`.
    scratch_root: PathBuf,
    /// `$HOME/.claude/projects`, the only root a persisted output file may
    /// resolve under.
    projects_root: PathBuf,
    /// The operator's uid, which must own the scratch directory and tickets.
    uid: u32,
    /// This process's pid, which must lie inside the session's process tree.
    pid: u32,
}

impl RelayEnv {
    /// Capture the snapshot, or name what makes it impossible.
    fn from_process() -> Result<Self, String> {
        let required = |name: &str| non_empty_env(name).ok_or_else(|| format!("{name} is not set"));
        let scratch_root = scratch_root_from_env().map_err(|error| format!("{error:#}"))?;
        let home = dirs::home_dir().ok_or("the home directory could not be determined")?;
        Ok(RelayEnv {
            session_id: required("LOOM_SESSION_ID")?,
            stage_id: required("LOOM_STAGE_ID")?,
            scratch_dir: PathBuf::from(required("LOOM_SCRATCH_DIR")?),
            scratch_root,
            projects_root: home.join(".claude").join("projects"),
            // SAFETY: `getuid` has no preconditions and cannot fail.
            uid: unsafe { libc::getuid() },
            pid: std::process::id(),
        })
    }
}

/// `LOOM_WORK_DIR`, canonicalized: inside a worktree it can name the `.work`
/// symlink, and the inbox writer opens its root without following one.
fn work_dir_from_env() -> Result<PathBuf, String> {
    let raw = non_empty_env("LOOM_WORK_DIR").ok_or("LOOM_WORK_DIR is not set")?;
    std::fs::canonicalize(&raw)
        .map_err(|error| format!("LOOM_WORK_DIR {raw} does not resolve: {error}"))
}

/// Relay every ticketed `LOOM_RELAY_V1` line in the PostToolUse payload on
/// stdin. Always `Ok(())`: a PostToolUse hook has nothing to fail, only
/// something to say, and every refusal is said in the reply.
pub fn relay(allowed_kinds: &str) -> Result<()> {
    let mut raw = Vec::new();
    let _ = std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES)
        .read_to_end(&mut raw);
    let allowed = parse_allowed_kinds(allowed_kinds);
    let context = RelayEnv::from_process()
        .and_then(|env| work_dir_from_env().map(|work_dir| (env, work_dir)));
    let reply = match context {
        Ok((env, work_dir)) => relay_payload(&raw, &env, &work_dir, &allowed),
        Err(reason) => unavailable(&raw, &reason),
    };
    if let Some(message) = reply {
        println!("{}", hook_reply(&message));
    }
    Ok(())
}

/// The kinds the hook derived from the Bash command. An unknown name is
/// dropped: the hook ships with this binary, so dropping can only narrow what
/// is relayed.
fn parse_allowed_kinds(csv: &str) -> Vec<RequestKind> {
    csv.split(',')
        .filter_map(|name| name.parse().ok())
        .collect()
}

/// Without a usable environment nothing can be relayed; say so only when the
/// inline output carries a relay line.
fn unavailable(raw: &[u8], reason: &str) -> Option<String> {
    let payload: Value = serde_json::from_slice(raw).ok()?;
    let lines = RelayLine::extract(&output::inline_text(&payload));
    (!lines.is_empty()).then(|| {
        format!(
            "LOOM relay: unavailable ({reason}), so nothing was relayed. Stop and report it; do not retry."
        )
    })
}

/// The testable core: relay the ticketed lines in `payload` for the session
/// `env` names, into the inbox under `work_dir`. `None` means print nothing.
fn relay_payload(
    payload: &[u8],
    env: &RelayEnv,
    work_dir: &Path,
    allowed: &[RequestKind],
) -> Option<String> {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return Some(
            "LOOM relay: the hook payload was not readable JSON, so nothing was relayed."
                .to_string(),
        );
    };
    if value.get("tool_name").and_then(Value::as_str) != Some("Bash") {
        return None;
    }
    let collected = output::collect(&value, &env.projects_root);
    let ticketed: Vec<RelayLine> = RelayLine::extract(&collected.text)
        .into_iter()
        .filter(|line| {
            ticket_path(&env.scratch_dir, line)
                .symlink_metadata()
                .is_ok()
        })
        .collect();

    let mut report = Vec::new();
    if let Some(refusal) = collected.persisted_refusal {
        if !allowed.is_empty() || !ticketed.is_empty() {
            report.push(refusal);
        }
    }
    if !ticketed.is_empty() {
        report.extend(relay_ticketed(&ticketed, &value, env, work_dir, allowed));
    }
    (!report.is_empty()).then(|| report.join("\n"))
}

/// Prove the session once, then relay each ticketed line on its own.
fn relay_ticketed(
    lines: &[RelayLine],
    payload: &Value,
    env: &RelayEnv,
    work_dir: &Path,
    allowed: &[RequestKind],
) -> Vec<String> {
    if let Err(reason) = prove_session(env, work_dir).and_then(|()| check_scratch_dir(env)) {
        let labels: Vec<String> = lines.iter().map(label).collect();
        return vec![format!(
            "LOOM relay: refused {}: {reason}",
            labels.join(", ")
        )];
    }
    let request = Request {
        env,
        work_dir,
        allowed,
        agent: agent_role(payload),
        tool_use_id: payload
            .get("tool_use_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    lines.iter().map(|line| request.relay_one(line)).collect()
}

/// This process must lie inside the session's own process tree, and the
/// session record must be Running for the stage the environment names.
fn prove_session(env: &RelayEnv, work_dir: &Path) -> Result<(), String> {
    if !caller_is_inside_session(work_dir, &env.session_id, env.pid) {
        return Err(format!(
            "this Bash call is not running inside session {}. A teammate outside the lead's \
             process tree cannot relay requests: send the note to the lead instead",
            env.session_id
        ));
    }
    match load_session_exact(work_dir, &env.session_id) {
        Ok(Some(session))
            if session.status == SessionStatus::Running
                && session.stage_id.as_deref() == Some(env.stage_id.as_str()) =>
        {
            Ok(())
        }
        Ok(_) => Err(format!(
            "session {} is not a Running session of stage {}",
            env.session_id, env.stage_id
        )),
        Err(error) => Err(format!(
            "session {} could not be read: {error:#}",
            env.session_id
        )),
    }
}

/// The scratch directory must be the one loom derives for this session: a
/// real 0700 directory the operator owns.
fn check_scratch_dir(env: &RelayEnv) -> Result<(), String> {
    let expected =
        session_dir(&env.scratch_root, &env.session_id).map_err(|error| format!("{error:#}"))?;
    if env.scratch_dir != expected {
        return Err(format!(
            "LOOM_SCRATCH_DIR {} is not this session's scratch directory {}",
            env.scratch_dir.display(),
            expected.display()
        ));
    }
    validate_session_dir(&env.scratch_dir, &env.session_id, env.uid)
        .map_err(|error| format!("{error:#}"))
}

/// One tool call's relay context, shared by every line it carries.
struct Request<'a> {
    env: &'a RelayEnv,
    work_dir: &'a Path,
    allowed: &'a [RequestKind],
    agent: AgentRole,
    tool_use_id: Option<String>,
}

impl Request<'_> {
    /// Admit, verify, write and consume one ticketed line; one reply line.
    fn relay_one(&self, line: &RelayLine) -> String {
        let label = label(line);
        if let Err(reason) = self.admit(line.kind) {
            return format!("LOOM relay: refused {label}: {reason}");
        }
        let ticket = match ticket_check::read_verified(&self.env.scratch_dir, line, self.env.uid) {
            Ok(ticket) => ticket,
            Err(error) => return format!("LOOM relay: refused {label}: {error:#}"),
        };
        let entry = InboxEntry {
            v: INBOX_ENTRY_VERSION,
            id: line.id.clone(),
            kind: line.kind,
            relayed_at: Utc::now(),
            session_id: self.env.session_id.clone(),
            stage_id: self.env.stage_id.clone(),
            agent: self.agent,
            tool_use_id: self.tool_use_id.clone(),
            payload: ticket.payload,
        };
        let ticket_file = ticket_path(&self.env.scratch_dir, line);
        match write_entry(self.work_dir, &entry) {
            Ok(WriteOutcome::Written) => format!(
                "LOOM relay: received {label} (the daemon applies it within ~5s){}",
                consume(&ticket_file)
            ),
            Ok(WriteOutcome::AlreadyRelayed) => format!(
                "LOOM relay: {label} was already relayed; nothing new was recorded{}",
                consume(&ticket_file)
            ),
            Ok(WriteOutcome::Capacity) => format!(
                "LOOM relay: refused {label}: the session inbox already holds \
                 {MAX_PENDING_ENTRIES} pending requests, so the daemon is not draining it. \
                 Stop and report it; do not retry."
            ),
            Err(error) => format!(
                "LOOM relay: failed to record {label}: {error:#}. Stop and report it; do not retry."
            ),
        }
    }

    /// Control kinds never come from a subagent, and a kind the Bash command
    /// did not run the writer for is not relayed.
    fn admit(&self, kind: RequestKind) -> Result<(), String> {
        if self.agent == AgentRole::Subagent && kind.is_control() {
            return Err(
                "control requests (block, dispute, handoff, merge-resolved, verdict) \
                        are never relayed from a subagent. Ask the main agent to run the command"
                    .to_string(),
            );
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

/// Remove a relayed ticket. A failure is reported, not fatal: relaying the
/// same id again is already a no-op.
fn consume(ticket: &Path) -> String {
    match std::fs::remove_file(ticket) {
        Ok(()) => String::new(),
        Err(error) => format!(
            " (its ticket {} could not be removed: {error})",
            ticket.display()
        ),
    }
}

fn ticket_path(scratch_dir: &Path, line: &RelayLine) -> PathBuf {
    scratch_dir.join(ticket_check::ticket_file_name(&line.id))
}

/// A payload carrying a non-empty `agent_type` came from a subagent.
fn agent_role(payload: &Value) -> AgentRole {
    match payload.get("agent_type").and_then(Value::as_str) {
        Some(agent_type) if !agent_type.is_empty() => AgentRole::Subagent,
        _ => AgentRole::Main,
    }
}

/// `<kind> <first 8 id characters>`, the id prefix the CLI prints.
fn label(line: &RelayLine) -> String {
    format!("{} {}", line.kind, &line.id[..8])
}

fn hook_reply(message: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": message,
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests;
