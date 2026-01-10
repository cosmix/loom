//! Context health tracking for sessions

use crate::models::constants::CONTEXT_WARNING_THRESHOLD;

/// Context health level for a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextHealth {
    Green,
    Yellow,
    Red,
}

/// Calculate context health from tokens
pub fn context_health(tokens: u32, limit: u32) -> ContextHealth {
    if limit == 0 {
        return ContextHealth::Green;
    }

    let usage = tokens as f32 / limit as f32;

    if usage >= CONTEXT_WARNING_THRESHOLD {
        ContextHealth::Red
    } else if usage >= 0.60 {
        ContextHealth::Yellow
    } else {
        ContextHealth::Green
    }
}

/// Calculate context usage percentage
pub fn context_usage_percent(tokens: u32, limit: u32) -> f32 {
    if limit == 0 {
        return 0.0;
    }

    (tokens as f32 / limit as f32) * 100.0
}
