//! Parse cumulative token counts and metadata from Codex rollout lines.

use crate::tokens::model::{optional_count, TokenUsage};
use chrono::DateTime;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodexTotals {
    pub input: u64,
    pub cached_input: u64,
    pub cache_write: u64,
    pub output: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexLine {
    TokenCount {
        timestamp: i64,
        total: CodexTotals,
    },
    TurnContext {
        model: String,
        effort: Option<String>,
    },
    SessionMeta {
        thread_id: String,
    },
    Other,
    Unreadable,
}

impl CodexTotals {
    /// Field-wise difference; None when any field decreased.
    pub fn delta_since(&self, previous: &CodexTotals) -> Option<CodexTotals> {
        Some(Self {
            input: self.input.checked_sub(previous.input)?,
            cached_input: self.cached_input.checked_sub(previous.cached_input)?,
            cache_write: self.cache_write.checked_sub(previous.cache_write)?,
            output: self.output.checked_sub(previous.output)?,
        })
    }

    /// Separate cached input subsets from fresh input, saturating at zero.
    pub fn to_usage(&self) -> TokenUsage {
        TokenUsage {
            input: self
                .input
                .saturating_sub(self.cached_input)
                .saturating_sub(self.cache_write),
            cache_write: self.cache_write,
            cache_read: self.cached_input,
            output: self.output,
        }
    }
}

pub fn parse_line(line: &str) -> CodexLine {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) if line.contains("token_count") => return CodexLine::Unreadable,
        Err(_) => return CodexLine::Other,
    };
    let payload = &value["payload"];
    match value["type"].as_str() {
        Some("event_msg") if payload["type"].as_str() == Some("token_count") => {
            if !payload["info"].is_object() {
                return CodexLine::Other;
            }
            parse_token_count(&value).unwrap_or(CodexLine::Unreadable)
        }
        Some("turn_context") => payload["model"]
            .as_str()
            .map(|model| CodexLine::TurnContext {
                model: model.to_owned(),
                effort: payload["effort"].as_str().map(str::to_owned),
            })
            .unwrap_or(CodexLine::Other),
        Some("session_meta") => payload["id"]
            .as_str()
            .map(|id| CodexLine::SessionMeta {
                thread_id: id.to_owned(),
            })
            .unwrap_or(CodexLine::Other),
        _ => CodexLine::Other,
    }
}

fn parse_token_count(value: &Value) -> Option<CodexLine> {
    let usage = &value["payload"]["info"]["total_token_usage"];
    let total = CodexTotals {
        input: usage["input_tokens"].as_u64()?,
        cached_input: optional_count(usage.get("cached_input_tokens"))?,
        cache_write: optional_count(usage.get("cache_write_input_tokens"))?,
        output: usage["output_tokens"].as_u64()?,
    };
    let timestamp = DateTime::parse_from_rfc3339(value["timestamp"].as_str()?)
        .ok()?
        .timestamp();
    Some(CodexLine::TokenCount { timestamp, total })
}

#[cfg(test)]
#[path = "codex_line_tests.rs"]
mod tests;
