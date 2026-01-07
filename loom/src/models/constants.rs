/// Default maximum context tokens for a runner session.
/// This represents the typical context window size for AI models.
pub const DEFAULT_CONTEXT_LIMIT: u32 = 200_000;

/// Context usage threshold (as a fraction of 1.0) at which to warn the user.
/// At 75% context usage, runners should consider creating a handoff.
pub const CONTEXT_WARNING_THRESHOLD: f32 = 0.75;

/// Context usage threshold (as a fraction of 1.0) that is considered critical.
/// At 85% context usage, runners must immediately create a handoff.
pub const CONTEXT_CRITICAL_THRESHOLD: f32 = 0.85;

/// Minimum valid priority value for signals.
pub const SIGNAL_MIN_PRIORITY: u8 = 1;

/// Maximum valid priority value for signals.
pub const SIGNAL_MAX_PRIORITY: u8 = 5;

/// Default priority value for new signals.
pub const SIGNAL_DEFAULT_PRIORITY: u8 = 3;

/// Context threshold percentages for display coloring.
pub mod display {
    /// Below this percentage, context usage is considered healthy (green).
    pub const CONTEXT_HEALTHY_PCT: f32 = 60.0;

    /// Between HEALTHY and WARNING, context is moderate (yellow).
    /// Above WARNING percentage, context usage is critical (red).
    pub const CONTEXT_WARNING_PCT: f32 = 75.0;

    /// Threshold for "critical" display in runner list view.
    pub const CONTEXT_CRITICAL_PCT: f32 = 85.0;
}
