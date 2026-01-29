/// Default maximum context tokens for a runner session.
/// This represents the typical context window size for AI models.
pub const DEFAULT_CONTEXT_LIMIT: u32 = 200_000;

/// Context usage threshold (as a fraction of 1.0) at which to warn the user.
/// At 50% context usage, runners should start preparing for handoff.
/// This is set lower than Claude Code's compaction threshold (~75-80%)
/// to ensure we capture context before automatic compaction occurs.
pub const CONTEXT_WARNING_THRESHOLD: f32 = 0.50;

/// Context usage threshold (as a fraction of 1.0) that triggers handoff.
/// At 65% context usage, runners must create a handoff immediately.
/// This provides a ~10% buffer before Claude Code's compaction (~75-80%).
pub const CONTEXT_CRITICAL_THRESHOLD: f32 = 0.65;

/// Default context budget percentage for stages.
/// Stages can override this in their definition.
pub const DEFAULT_CONTEXT_BUDGET: f32 = 65.0;

/// Hard limit context percentage that triggers forced handoff.
/// Even if a stage sets a higher budget, this is the absolute max.
pub const CONTEXT_ABSOLUTE_MAX: f32 = 75.0;

/// Context threshold percentages for display coloring.
pub mod display {
    /// Below this percentage, context usage is considered healthy (green).
    pub const CONTEXT_HEALTHY_PCT: f32 = 50.0;

    /// Between HEALTHY and WARNING, context is moderate (yellow).
    /// Above WARNING percentage, context usage is critical (red).
    pub const CONTEXT_WARNING_PCT: f32 = 65.0;
}

/// Staleness threshold in seconds for session heartbeats.
/// When a session hasn't sent a heartbeat for this duration,
/// it is considered stale (possibly hung).
pub const STALENESS_THRESHOLD_SECS: u64 = 300; // 5 minutes
