//! Transcript and rollout builders carrying model and effort keys, shaped from
//! the line formats in `architecture/token-flow-sources.md` "Token Usage Sources".

use crate::support::{body, iso, Fx};
use loom::tokens::{ModelUsage, TokenUsage};
use serde_json::{json, Map, Value};
use std::fs;

pub const SOL_MODEL: &str = "gpt-6.1-sol";

/// A Claude assistant line; `effort` and `per_turn` are left out when `None`.
pub fn assistant(
    msg_id: &str,
    ts: i64,
    model: &str,
    effort: Option<&str>,
    per_turn: Option<&str>,
    u: &TokenUsage,
) -> String {
    let mut line = Map::new();
    line.insert("type".into(), json!("assistant"));
    line.insert("timestamp".into(), json!(iso(ts)));
    line.insert(
        "sessionId".into(),
        json!("01ed9f4c-608e-4e2c-8edf-05d26adcd4d6"),
    );
    if let Some(effort) = effort {
        line.insert("effort".into(), json!(effort));
    }
    if let Some(per_turn) = per_turn {
        line.insert("perTurnEffort".into(), json!(per_turn));
    }
    let usage = json!({
        "input_tokens": u.input,
        "cache_creation_input_tokens": u.cache_write,
        "cache_read_input_tokens": u.cache_read,
        "output_tokens": u.output,
    });
    line.insert(
        "message".into(),
        json!({"id": msg_id, "model": model, "usage": usage}),
    );
    Value::Object(line).to_string()
}

/// A rollout for `thread_id`: session_meta, turn_context naming `SOL_MODEL` at
/// `effort`, then one token_count whose cumulative total is
/// `[input, cached_input, output]` (cache_write 0).
pub fn add_effort_rollout(fx: &Fx, thread_id: &str, ts: i64, effort: &str, total: [u64; 3]) {
    let dir = fx.path("codex-sessions/2026/10/14");
    fs::create_dir_all(&dir).expect("rollout dir");
    let [input, cached, output] = total;
    let usage = json!({"input_tokens": input, "cached_input_tokens": cached,
        "cache_write_input_tokens": 0, "output_tokens": output,
        "reasoning_output_tokens": 0, "total_tokens": input + output});
    let lines = [
        json!({"timestamp": iso(ts - 8), "type": "session_meta",
            "payload": {"id": thread_id, "cwd": "/tmp", "originator": "codex-tui"}})
        .to_string(),
        json!({"timestamp": iso(ts - 4), "type": "turn_context",
            "payload": {"model": SOL_MODEL, "effort": effort}})
        .to_string(),
        json!({"timestamp": iso(ts), "type": "event_msg", "payload": {"type": "token_count",
            "info": {"total_token_usage": usage, "last_token_usage": usage,
                "model_context_window": 475000}}})
        .to_string(),
    ];
    let name = format!("rollout-2026-10-14T10-58-07-{thread_id}.jsonl");
    fs::write(dir.join(name), body(&lines)).expect("rollout file");
}

/// A models entry with an effort.
pub fn model_entry(model: &str, effort: &str, total: TokenUsage) -> ModelUsage {
    ModelUsage {
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        total,
    }
}

/// Field-wise sum of every models entry.
pub fn models_sum(models: &[ModelUsage]) -> TokenUsage {
    let mut sum = TokenUsage::default();
    for entry in models {
        sum.add(&entry.total);
    }
    sum
}
