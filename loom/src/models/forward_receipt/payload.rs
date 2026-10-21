//! A forward's PostToolUse payload as lifecycle evidence. Claude Code writes a
//! call's `tool_result` row to the transcript only after the call's PostToolUse
//! hooks finish, so the payload's own `tool_response` is the only place the
//! result exists while the hook runs.

use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::is_safe_id;
use super::transcript::{collect_strings, forward_argv, ForwardingInvocation, ForwardingResult};

/// The forward a payload describes, with its result attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadForward {
    pub parent_session_id: String,
    pub agent_id: String,
    pub invocation: ForwardingInvocation,
}

/// Decode `raw`, the hook's stdin. Every failure names the offending field and
/// never echoes payload text.
pub fn decode_payload(raw: &str, now: DateTime<Utc>) -> Result<PayloadForward> {
    let Ok(Value::Object(payload)) = serde_json::from_str::<Value>(raw) else {
        bail!("payload is not a JSON object");
    };
    ensure!(
        payload.get("tool_name").and_then(Value::as_str) == Some("Bash"),
        "payload is not a Bash call"
    );
    let command = payload
        .get("tool_input")
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str);
    let Some((model, effort)) = command.and_then(forward_argv) else {
        bail!("payload command is not a forward");
    };
    let tool_use_id = safe_id(&payload, "tool_use_id")?;
    // Only forwarder subagents run forwards, so a payload without `agent_id`
    // (a main-session call) is refused and forms no receipt;
    // `codex-forward-result.sh` requires `.agent_id` the same way.
    let agent_id = safe_id(&payload, "agent_id")?;
    let parent_session_id = safe_id(&payload, "session_id")?;
    // A call without a result has nothing to observe; the hook writes nothing for it.
    let result = match payload
        .get("tool_response")
        .or_else(|| payload.get("tool_result"))
    {
        Some(Value::Object(response)) => Some(result_from(response, &tool_use_id, now)),
        None | Some(Value::Null) => None,
        Some(_) => bail!("payload tool result is not an object"),
    };
    Ok(PayloadForward {
        parent_session_id,
        agent_id,
        invocation: ForwardingInvocation {
            id: tool_use_id,
            model,
            effort,
            timestamp: now,
            result,
        },
    })
}

fn safe_id(payload: &Map<String, Value>, field: &str) -> Result<String> {
    match payload.get(field).and_then(Value::as_str) {
        Some(value) if is_safe_id(value) => Ok(value.to_owned()),
        _ => bail!("payload field {field} is missing or unsafe"),
    }
}

fn result_from(
    response: &Map<String, Value>,
    tool_use_id: &str,
    now: DateTime<Utc>,
) -> ForwardingResult {
    let text = ["stdout", "output"]
        .iter()
        .find_map(|key| response.get(*key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned();
    let background_task_id = ["backgroundTaskId", "background_task_id"]
        .iter()
        .find_map(|key| response.get(*key).and_then(Value::as_str))
        .filter(|id| is_safe_id(id))
        .map(str::to_owned);
    let mut task_output_path_text = vec![text.clone()];
    collect_strings(&Value::Object(response.clone()), &mut task_output_path_text);
    ForwardingResult {
        id: tool_use_id.to_owned(),
        text,
        background_task_id,
        task_output_path_text,
        timestamp: now,
    }
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
