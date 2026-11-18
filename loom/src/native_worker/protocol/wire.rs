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

pub fn thread_start(id: u64, model: &str, cwd: &Path, developer_instructions: &str) -> Value {
    json!({"id": id, "method": "thread/start", "params": {
        "model": model, "cwd": cwd.to_string_lossy(), "approvalPolicy": "never",
        "sandbox": "workspace-write", "developerInstructions": developer_instructions
    }})
}

pub fn turn_start(
    id: u64,
    thread_id: &str,
    texts: &[String],
    effort: &str,
    worktree: &Path,
) -> Value {
    let input: Vec<Value> = texts
        .iter()
        .map(|text| json!({"type": "text", "text": text}))
        .collect();
    // Codex's own sandbox runs every command: writes only to the worktree and
    // `TMPDIR`. Codex adds `$TMPDIR` of its own process environment to the
    // writable roots. The boundary sets it to `BOUNDARY_TMPDIR`, a private empty
    // directory linkers and compilers need. `/tmp` itself stays read-only
    // to sandboxed commands: binds below it (the Cargo scratch, a worktree
    // under `/tmp`) must not be renamed away and replaced. Without network
    // Codex's seccomp filter refuses AF_INET sockets and AF_UNIX
    // `connect`, so commands reach neither the bridge's loopback port nor
    // the proxy socket; only the app-server's own requests use the proxy.
    let policy = json!({"type": "workspaceWrite", "writableRoots": [worktree.to_string_lossy()],
        "networkAccess": false, "excludeSlashTmp": true, "excludeTmpdirEnvVar": false});
    json!({"id": id, "method": "turn/start", "params": {
        "threadId": thread_id, "input": input, "effort": effort, "sandboxPolicy": policy
    }})
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
