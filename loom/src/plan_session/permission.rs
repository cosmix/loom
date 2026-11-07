//! The blocking `PermissionRequest` hook and the answer file the server writes.
//!
//! The hook appends the request to the spool under a fresh `request_id`, then polls
//! `answers/<request_id>.json` until the server answers through [`write_answer`]. Every failure
//! is a deny: a hook that prints nothing or dies would leave Claude Code to show its own dialog
//! in a pane nobody watches.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::guard::load_policy;
use super::kind::SessionKind;
use super::model::DenyReason;
use super::spool::{append_event, read_payload};
use crate::fs::safe_fs::{
    safe_create_new_in_workdir, safe_open_dirfd, safe_remove_in_workdir, safe_rename_in_workdir,
};
use crate::fs::safe_read::{is_not_found, read_bounded};

/// How long the hook waits for an answer: under Claude Code's 3600 s hook timeout (a hook killed
/// by its timeout is a non-blocking error) and over the server's own 3590 s expiry.
pub const HOOK_WAIT: Duration = Duration::from_secs(3595);
/// How often the hook looks for the answer file.
pub const POLL: Duration = Duration::from_millis(200);

const EVENT: &str = "PermissionRequest";
const ANSWERS: &str = "answers";
/// An answer is three short fields; [`write_answer`] keeps every answer within this bound.
const MAX_ANSWER_BYTES: usize = 4 * 1024;
const SUMMARY_DENY: &str =
    "summary sessions have no one to ask; their rules are wrong if this appears";

/// The server's decision on one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    Allow,
    Deny,
}

/// The answer file `answers/<request_id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub behavior: Behavior,
    /// For a deny, the text Claude Code reads (the holder's note).
    pub message: Option<String>,
    pub reason: Option<DenyReason>,
}

/// Write the answer for `request_id` (a lowercase hyphenated uuid; it becomes a file name)
/// through a temp file and a rename, so the hook never reads a partial answer.
pub fn write_answer(session_dir: &Path, request_id: &str, answer: &Answer) -> anyhow::Result<()> {
    check_request_id(request_id)?;
    let bytes = encode_within_bound(answer)?;
    let dir = safe_open_dirfd(session_dir)?;
    let temp = PathBuf::from(format!("{ANSWERS}/.{request_id}.tmp"));
    let target = PathBuf::from(format!("{ANSWERS}/{request_id}.json"));
    // A temp file a crashed writer left behind would fail the exclusive create; none is the
    // normal case, so the removal's error is not one.
    let _ = safe_remove_in_workdir(dir.as_raw_fd(), &temp);
    safe_create_new_in_workdir(dir.as_raw_fd(), &temp, &bytes)
        .with_context(|| format!("cannot write the answer for {request_id}"))?;
    safe_rename_in_workdir(dir.as_raw_fd(), &temp, &target)
        .with_context(|| format!("cannot publish the answer for {request_id}"))
}

fn check_request_id(request_id: &str) -> anyhow::Result<()> {
    let canonical = uuid::Uuid::try_parse(request_id)
        .map(|id| id.hyphenated().to_string())
        .ok();
    ensure!(
        canonical.as_deref() == Some(request_id),
        "permission request id {request_id:?} is not a lowercase hyphenated uuid"
    );
    Ok(())
}

/// Serialise `answer`, cutting a message too long for the hook's read bound on a char boundary
/// and marking the cut with `…`.
fn encode_within_bound(answer: &Answer) -> anyhow::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(answer)?;
    if bytes.len() <= MAX_ANSWER_BYTES {
        return Ok(bytes);
    }
    let full = answer
        .message
        .as_deref()
        .context("an answer without a message is small")?;
    let mut cut = Answer {
        message: Some("…".into()),
        ..answer.clone()
    };
    let budget = MAX_ANSWER_BYTES
        .checked_sub(serde_json::to_vec(&cut)?.len())
        .context("the answer does not fit the answer file bound")?;
    let mut used = 0;
    let mut kept: String = full
        .chars()
        .take_while(|&c| {
            used += escaped_len(c);
            used <= budget
        })
        .collect();
    kept.push('…');
    cut.message = Some(kept);
    let bytes = serde_json::to_vec(&cut)?;
    ensure!(
        bytes.len() <= MAX_ANSWER_BYTES,
        "the answer does not fit the answer file bound"
    );
    Ok(bytes)
}

/// The bytes `serde_json` writes for `c` inside a string.
fn escaped_len(c: char) -> usize {
    match c {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
        c if u32::from(c) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

/// The hook side of `loom plan hook PermissionRequest`: prints one decision line, or nothing
/// for another event's payload.
pub fn run_hook(session_dir: &Path, stdin: impl Read, stdout: impl Write) {
    run_hook_with(session_dir, stdin, stdout, HOOK_WAIT, POLL);
}

/// [`run_hook`] with the wait and the poll interval chosen by the caller (tests).
pub(crate) fn run_hook_with(
    session_dir: &Path,
    stdin: impl Read,
    mut stdout: impl Write,
    wait: Duration,
    poll: Duration,
) {
    let verdict = catch_unwind(AssertUnwindSafe(|| decide(session_dir, stdin, wait, poll)))
        .unwrap_or_else(|_| Some(hook_error("the permission hook failed")));
    if let Some(verdict) = verdict {
        let _ = writeln!(stdout, "{}", verdict.render());
        let _ = stdout.flush();
    }
}

/// What the hook tells Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Allow,
    Deny(String),
}

impl Verdict {
    /// The one-line answer; the message goes through `serde_json`, never string formatting.
    fn render(&self) -> String {
        let decision = match self {
            Self::Allow => json!({"behavior": "allow"}),
            Self::Deny(message) => json!({"behavior": "deny", "message": message}),
        };
        json!({"hookSpecificOutput": {"hookEventName": EVENT, "decision": decision}}).to_string()
    }
}

fn hook_error(why: impl std::fmt::Display) -> Verdict {
    Verdict::Deny(format!("loom plan: {why}"))
}

fn decide(session_dir: &Path, stdin: impl Read, wait: Duration, poll: Duration) -> Option<Verdict> {
    let mut payload = match request_payload(stdin) {
        Ok(Some(payload)) => payload,
        Ok(None) => return None,
        Err(verdict) => return Some(verdict),
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    payload.insert("request_id".into(), Value::String(request_id.clone()));
    let kind = load_policy(session_dir).map(|policy| policy.kind);
    let auto_denied = match &kind {
        Ok(SessionKind::Summary) => Some(DenyReason::SummarySession),
        Ok(_) => None,
        Err(_) => Some(DenyReason::HookError),
    };
    if let Some(reason) = auto_denied {
        // Logged for the chat, then answered here: nobody is asked.
        payload.insert("auto_denied".into(), json!(reason));
        let _ = append_event(session_dir, EVENT, &Value::Object(payload));
        return Some(match kind {
            Ok(_) => Verdict::Deny(SUMMARY_DENY.into()),
            Err(error) => hook_error(format!("cannot load the guard policy: {error:#}")),
        });
    }
    if let Err(error) = append_event(session_dir, EVENT, &Value::Object(payload)) {
        return Some(hook_error(format!("cannot record the request: {error:#}")));
    }
    Some(wait_for_answer(session_dir, &request_id, wait, poll))
}

/// The payload object; `Ok(None)` for another event's payload (the spool lesson,
/// `mistakes/spurious-waiting-for-input.md`), a deny for anything unusable.
fn request_payload(stdin: impl Read) -> Result<Option<Map<String, Value>>, Verdict> {
    let Some(payload) = read_payload(stdin) else {
        return Err(hook_error("the request is not JSON or is over 1 MiB"));
    };
    let Value::Object(fields) = payload else {
        return Err(hook_error("the request is not a JSON object"));
    };
    match fields.get("hook_event_name") {
        None | Some(Value::Null) => Ok(Some(fields)),
        Some(Value::String(name)) if name == EVENT => Ok(Some(fields)),
        Some(Value::String(_)) => Ok(None),
        Some(_) => Err(hook_error("hook_event_name is not a string")),
    }
}

fn wait_for_answer(
    session_dir: &Path,
    request_id: &str,
    wait: Duration,
    poll: Duration,
) -> Verdict {
    let deadline = Instant::now() + wait;
    let answer = PathBuf::from(format!("{ANSWERS}/{request_id}.json"));
    loop {
        // The bridge removes the directory when the session ends: nobody will answer.
        if !is_directory(session_dir) || !is_directory(&session_dir.join(ANSWERS)) {
            return hook_error("the session ended before an answer arrived");
        }
        match read_bounded(session_dir, &answer, MAX_ANSWER_BYTES) {
            Ok(bytes) => return verdict_of(&bytes),
            Err(error) if is_not_found(&error) => {}
            Err(error) => return hook_error(format!("the answer is unreadable: {error:#}")),
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return hook_error("no answer arrived in time");
        }
        std::thread::sleep(poll.min(left));
    }
}

/// A real directory, never a symlink to one.
fn is_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn verdict_of(bytes: &[u8]) -> Verdict {
    let answer: Answer = match serde_json::from_slice(bytes) {
        Ok(answer) => answer,
        Err(error) => return hook_error(format!("the answer is unparsable: {error}")),
    };
    match answer.behavior {
        Behavior::Allow => Verdict::Allow,
        Behavior::Deny => {
            let message = answer.message.filter(|note| !note.trim().is_empty());
            Verdict::Deny(message.unwrap_or_else(|| deny_text(answer.reason).into()))
        }
    }
}

/// The message of a deny the server wrote without a note.
fn deny_text(reason: Option<DenyReason>) -> &'static str {
    match reason {
        None | Some(DenyReason::Holder) => "denied in the loom plan chat",
        Some(DenyReason::Timeout) => "nobody answered in the loom plan chat in time",
        Some(DenyReason::SessionEnded) => "the loom plan session no longer waits for this answer",
        Some(DenyReason::ServerStopping) => "the loom plan server is stopping",
        Some(DenyReason::HookError) => "loom plan: the permission hook failed",
        Some(DenyReason::Interrupt) => "the request was interrupted in the loom plan chat",
        Some(DenyReason::SummarySession) => SUMMARY_DENY,
        Some(DenyReason::Settings) => super::web_fetch::TURNED_OFF,
    }
}

#[cfg(test)]
#[path = "permission_tests.rs"]
mod tests;
