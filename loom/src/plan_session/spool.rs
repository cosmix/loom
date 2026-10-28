//! The hook event spool: the hook-side append and the server-side reader.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use super::sandbox::HookSpec;
use crate::fs::safe_fs::safe_append;
use crate::models::dispute::truncate_to_byte_limit;
use crate::tokens::tail::{self, FileStat, Line};

const MAX_PAYLOAD_BYTES: u64 = 1024 * 1024;
const MAX_STRING_BYTES: usize = 64 * 1024;
/// What ends a payload string cut at `MAX_STRING_BYTES`.
pub(crate) const TRUNCATED: &str = "…[truncated]";

/// Hook registrations installed beside the session guard.
pub const SPOOL_EVENTS: [HookSpec; 9] = [
    HookSpec {
        event: "SessionStart",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "UserPromptSubmit",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "PermissionRequest",
        matcher: Some("*"),
        timeout_secs: Some(3600),
    },
    HookSpec {
        event: "Notification",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "Stop",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "SubagentStart",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "SubagentStop",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "PreCompact",
        matcher: None,
        timeout_secs: None,
    },
    HookSpec {
        event: "SessionEnd",
        matcher: None,
        timeout_secs: None,
    },
];

/// A complete event received by a session hook.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SpoolEvent {
    pub received_at: DateTime<Utc>,
    pub event: String,
    pub session_id: Option<String>,
    pub transcript_path: Option<PathBuf>,
    pub request_id: Option<String>,
    pub payload: Value,
}

impl SpoolEvent {
    /// The hook answered this request itself (`auto_denied` set): nobody waits on it.
    pub(crate) fn auto_denied(&self) -> bool {
        self.payload
            .get("auto_denied")
            .is_some_and(|tag| !tag.is_null())
    }
}

/// Appends one JSON line under the safe filesystem's exclusive append lock.
pub fn append_event(session_dir: &Path, event: &str, payload: &Value) -> anyhow::Result<()> {
    let mut payload = payload.clone();
    truncate_strings(&mut payload);
    let mut entry = json!({
        "received_at": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        "event": event,
        "session_id": payload.get("session_id").and_then(Value::as_str),
        "transcript_path": payload.get("transcript_path").and_then(Value::as_str),
        "payload": payload,
    });
    if event == "PermissionRequest" {
        entry["request_id"] = payload
            .get("request_id")
            .and_then(Value::as_str)
            .map_or(Value::Null, |id| Value::String(id.to_owned()));
    }
    let mut line = serde_json::to_vec(&entry)?;
    line.push(b'\n');
    safe_append(session_dir, Path::new("events.jsonl"), &line)
}

fn truncate_strings(value: &mut Value) {
    match value {
        Value::String(text) if text.len() > MAX_STRING_BYTES => {
            *text = truncate_to_byte_limit(text, MAX_STRING_BYTES);
            text.push_str(TRUNCATED);
        }
        Value::Array(values) => values.iter_mut().for_each(truncate_strings),
        Value::Object(fields) => fields.values_mut().for_each(truncate_strings),
        _ => {}
    }
}

/// Reads at most 1 MiB plus one sentinel byte, rejecting oversized or invalid JSON.
pub(crate) fn read_payload(stdin: impl Read) -> Option<Value> {
    try_read_payload(stdin).ok()
}

/// [`read_payload`] with the reason a payload was refused, for a hook that reports it.
pub(crate) fn try_read_payload(stdin: impl Read) -> Result<Value, String> {
    let mut raw = Vec::new();
    stdin
        .take(MAX_PAYLOAD_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|error| format!("cannot read the payload: {error}"))?;
    if raw.len() as u64 > MAX_PAYLOAD_BYTES {
        return Err("the payload is over 1 MiB".into());
    }
    serde_json::from_slice(&raw).map_err(|error| format!("the payload is not JSON: {error}"))
}

/// A file tailed line by line; a shrunk or replaced file is read again from the start.
pub(crate) struct Tail {
    path: PathBuf,
    offset: u64,
    previous: Option<FileStat>,
    was_reset: bool,
}

impl Tail {
    /// Starts at the beginning of the file at `path`.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            previous: None,
            was_reset: false,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the last read restarted after a shrink or an inode change.
    pub(crate) fn was_reset(&self) -> bool {
        self.was_reset
    }

    /// Hands `on_line` each complete line the file gained; a missing file reads nothing.
    pub(crate) fn read_lines(&mut self, on_line: impl FnMut(Line<'_>)) -> std::io::Result<()> {
        self.was_reset = false;
        let Some(stat) = tail::stat(&self.path) else {
            return Ok(());
        };
        if self.previous.is_some_and(|old| {
            old.inode != stat.inode || stat.len < old.len || stat.len < self.offset
        }) {
            self.offset = 0;
            self.was_reset = true;
        }
        self.previous = Some(stat);
        let stop = AtomicBool::new(false);
        tail::read_lines(&self.path, &mut self.offset, stat.len, &stop, on_line)
    }
}

/// Incrementally reads complete spool lines; replay de-duplication belongs to the bridge.
pub struct SpoolReader {
    tail: Tail,
}

impl SpoolReader {
    /// Starts at the beginning of the supplied `events.jsonl` file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            tail: Tail::new(path),
        }
    }

    /// Whether the last read restarted after a shrink or an inode change.
    pub fn was_reset(&self) -> bool {
        self.tail.was_reset()
    }

    /// Returns newly completed events, skipping malformed lines with a warning.
    pub fn read_new(&mut self) -> Vec<SpoolEvent> {
        let mut events = Vec::new();
        let read = self.tail.read_lines(|line| match line {
            Line::Text(text) => match serde_json::from_str(text) {
                Ok(event) => events.push(event),
                Err(error) => tracing::warn!(%error, "skipping malformed hook spool line"),
            },
            Line::TooLong => tracing::warn!("skipping oversized hook spool line"),
        });
        if let Err(error) = read {
            let path = self.tail.path().display();
            tracing::warn!(path = %path, %error, "cannot read hook spool");
        }
        events
    }
}

#[cfg(test)]
#[path = "spool_tests.rs"]
mod tests;
