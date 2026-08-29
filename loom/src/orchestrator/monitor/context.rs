//! Context health tracking for sessions

/// Context health level for a session, as a fraction of the ceiling that
/// governs it.
///
/// The ceiling is an absolute token count resolved per stage
/// (`stage.context_ceiling_tokens` → `[context] ceiling_tokens` →
/// `DEFAULT_CONTEXT_CEILING_TOKENS`), so these bands describe how much of a
/// stage's own allowance is spent, not how full some model's window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextHealth {
    /// Below 60% of the ceiling — healthy operation
    Green,
    /// 60% to under 90% of the ceiling — prepare for handoff
    Yellow,
    /// 90% of the ceiling or more — handoff required immediately
    Red,
}

/// Bucket a session's resident tokens against the ceiling governing it.
///
/// A ceiling of 0 yields `Green`: an unresolvable ceiling is missing evidence,
/// and reporting missing evidence as an emergency would hand off every session
/// whose stage could not be loaded.
pub fn context_health(tokens: u32, ceiling: u32) -> ContextHealth {
    if ceiling == 0 {
        return ContextHealth::Green;
    }

    let usage = tokens as f32 / ceiling as f32;

    if usage >= 0.90 {
        ContextHealth::Red
    } else if usage >= 0.60 {
        ContextHealth::Yellow
    } else {
        ContextHealth::Green
    }
}
