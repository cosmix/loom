//! Builders and parsers for app-server JSON frames.

use std::path::Path;

use serde_json::{json, Value};

pub const DEVELOPER_INSTRUCTIONS: &str = "You are a loom implementation worker. Do not run git. Do not read or write any path under .loom or .work. Do not ask questions; if the brief is impossible, say why in your final answer and stop.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReply {
    Result(Value),
    Error { code: i64, message: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessage {
    pub id: String,
    pub text: String,
    pub phase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    TurnStarted {
        thread_id: String,
        turn_id: String,
    },
    TurnCompleted {
        thread_id: String,
        turn_id: String,
        status: String,
        error: Option<String>,
    },
    ItemCompleted {
        thread_id: String,
        turn_id: String,
        message: Option<AgentMessage>,
    },
    Error {
        thread_id: Option<String>,
        message: String,
        will_retry: bool,
    },
    Other,
}

pub fn initialize(id: u64, client_version: &str) -> Value {
    json!({"id": id, "method": "initialize", "params": {
        "clientInfo": {"name": "loom", "title": "loom", "version": client_version},
        "capabilities": {"optOutNotificationMethods": ["thread/tokenUsage/updated", "item/agentMessage/delta"]}
    }})
}

pub fn initialized() -> Value {
    json!({"method": "initialized", "params": {}})
}

/// The Codex sandbox the protocol asks for, fixed by how the boundary was
/// launched. Linux keeps Codex's own sandbox inside bubblewrap
/// (`WorkspaceWrite`). macOS refuses a Seatbelt profile started inside
/// another one, so Codex's per-command sandbox cannot run inside loom's
/// profile: `DangerFullAccess` there means loom's profile is the only
/// boundary, and only `SpawnedBoundary::activate` returns it, after the
/// trusted bridge confirmed it runs inside that profile. A machine cannot be
/// built without a mode (`MachineConfig::sandbox`), so no `thread/start` is
/// sent for a launch that confirmed none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    WorkspaceWrite,
    DangerFullAccess,
}

impl SandboxMode {
    /// The `thread/start` `sandbox` value.
    fn thread_sandbox(self) -> &'static str {
        match self {
            Self::WorkspaceWrite => "workspace-write",
            Self::DangerFullAccess => "danger-full-access",
        }
    }
}

pub fn thread_start(
    id: u64,
    model: &str,
    cwd: &Path,
    developer_instructions: &str,
    mode: SandboxMode,
) -> Value {
    let mut frame = bare_thread_start(id, cwd, mode);
    frame["params"]["model"] = json!(model);
    frame["params"]["developerInstructions"] = json!(developer_instructions);
    frame
}

/// A `thread/start` with Codex's default model and no instructions: the
/// session operation the macOS qualification probe sends offline.
pub fn bare_thread_start(id: u64, cwd: &Path, mode: SandboxMode) -> Value {
    json!({"id": id, "method": "thread/start", "params": {
        "cwd": cwd.to_string_lossy(), "approvalPolicy": "never",
        "sandbox": mode.thread_sandbox()
    }})
}

pub fn turn_start(
    id: u64,
    thread_id: &str,
    texts: &[String],
    effort: &str,
    worktree: &Path,
    mode: SandboxMode,
) -> Value {
    let input: Vec<Value> = texts
        .iter()
        .map(|text| json!({"type": "text", "text": text}))
        .collect();
    json!({"id": id, "method": "turn/start", "params": {
        "threadId": thread_id, "input": input, "effort": effort,
        "sandboxPolicy": turn_policy(worktree, mode)
    }})
}

/// The turn's `sandboxPolicy` for `mode`.
fn turn_policy(worktree: &Path, mode: SandboxMode) -> Value {
    match mode {
        // Codex's own sandbox runs every command: writes only to the worktree and
        // `TMPDIR`. Codex adds `$TMPDIR` of its own process environment to the
        // writable roots. The boundary sets it to `BOUNDARY_TMPDIR`, a private empty
        // directory linkers and compilers need. `/tmp` itself stays read-only
        // to sandboxed commands: binds below it (the Cargo scratch, a worktree
        // under `/tmp`) must not be renamed away and replaced. Without network
        // Codex's seccomp filter refuses AF_INET sockets and AF_UNIX
        // `connect`, so commands reach neither the bridge's loopback port nor
        // the proxy socket; only the app-server's own requests use the proxy.
        SandboxMode::WorkspaceWrite => {
            json!({"type": "workspaceWrite", "writableRoots": [worktree.to_string_lossy()],
                "networkAccess": false, "excludeSlashTmp": true, "excludeTmpdirEnvVar": false})
        }
        // Commands share loom's Seatbelt profile with the app-server: Codex
        // cannot nest a second profile inside it.
        SandboxMode::DangerFullAccess => json!({"type": "dangerFullAccess"}),
    }
}

pub fn turn_steer(id: u64, thread_id: &str, expected_turn_id: &str, text: &str) -> Value {
    json!({"id": id, "method": "turn/steer", "params": {
        "threadId": thread_id, "expectedTurnId": expected_turn_id, "input": [{"type": "text", "text": text}]
    }})
}

pub fn turn_interrupt(id: u64, thread_id: &str, turn_id: &str) -> Value {
    json!({"id": id, "method": "turn/interrupt", "params": {"threadId": thread_id, "turnId": turn_id}})
}

pub fn server_reply(request_id: &Value, reply: &ServerReply) -> Value {
    match reply {
        ServerReply::Result(value) => json!({"id": request_id, "result": value}),
        ServerReply::Error { code, message } => {
            json!({"id": request_id, "error": {"code": code, "message": message}})
        }
    }
}

pub fn thread_id_from_result(result: &Value) -> Option<String> {
    result.pointer("/thread/id")?.as_str().map(str::to_owned)
}

pub fn turn_id_from_result(result: &Value) -> Option<String> {
    result.pointer("/turn/id")?.as_str().map(str::to_owned)
}

fn string_at(value: &Value, pointer: &str) -> Option<String> {
    value.pointer(pointer)?.as_str().map(str::to_owned)
}

fn agent_message(item: &Value) -> Option<AgentMessage> {
    if item.get("type")?.as_str()? != "agentMessage" {
        return None;
    }
    Some(AgentMessage {
        id: string_at(item, "/id")?,
        text: string_at(item, "/text")?,
        phase: string_at(item, "/phase"),
    })
}

fn parse_known_notification(method: &str, params: &Value) -> Option<Notification> {
    match method {
        "turn/started" => Some(Notification::TurnStarted {
            thread_id: string_at(params, "/threadId")?,
            turn_id: string_at(params, "/turn/id")?,
        }),
        "turn/completed" => Some(Notification::TurnCompleted {
            thread_id: string_at(params, "/threadId")?,
            turn_id: string_at(params, "/turn/id")?,
            status: string_at(params, "/turn/status")?,
            error: string_at(params, "/turn/error/message"),
        }),
        "item/completed" => Some(Notification::ItemCompleted {
            thread_id: string_at(params, "/threadId")?,
            turn_id: string_at(params, "/turnId")?,
            message: agent_message(params.get("item")?),
        }),
        "error" => Some(Notification::Error {
            thread_id: string_at(params, "/threadId"),
            message: string_at(params, "/error/message")?,
            will_retry: params
                .get("willRetry")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        _ => None,
    }
}

pub fn parse_notification(method: &str, params: &Value) -> Notification {
    parse_known_notification(method, params).unwrap_or(Notification::Other)
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
