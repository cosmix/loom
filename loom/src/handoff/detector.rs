//! Context threshold detection for session handoffs.
//!
//! This module provides utilities to detect when a session's context usage
//! is approaching exhaustion and should trigger a handoff to a fresh session.

use crate::models::constants::{CONTEXT_CRITICAL_THRESHOLD, CONTEXT_WARNING_THRESHOLD};
use crate::models::session::Session;

/// Context usage level categorization.
///
/// Mirrors the ContextHealth enum from orchestrator::monitor but provides
/// a more explicit API for handoff-specific logic.
///
/// Thresholds are set to trigger handoff BEFORE Claude Code's automatic
/// context compaction (~75-80%), ensuring we capture full context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextLevel {
    /// Context usage is below 50% - healthy operation
    Green,
    /// Context usage is 50-64% - prepare handoff soon
    Yellow,
    /// Context usage is 65% or above - handoff required immediately
    Red,
}

/// Configuration for context threshold detection.
///
/// Allows customization of when warnings and handoffs are triggered.
/// Default thresholds are set to trigger BEFORE Claude Code's automatic
/// context compaction (~75-80%).
#[derive(Debug, Clone)]
pub struct ThresholdConfig {
    /// Fraction of context limit at which to show warning (default 0.50)
    pub warning_threshold: f32,
    /// Fraction of context limit at which handoff is required (default 0.65)
    pub critical_threshold: f32,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            warning_threshold: CONTEXT_WARNING_THRESHOLD,
            critical_threshold: CONTEXT_CRITICAL_THRESHOLD,
        }
    }
}

impl ThresholdConfig {
    /// Create a new threshold configuration with custom values.
    ///
    /// # Panics
    /// Panics if warning_threshold >= critical_threshold or if either is outside [0.0, 1.0]
    pub fn new(warning_threshold: f32, critical_threshold: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&warning_threshold),
            "warning_threshold must be between 0.0 and 1.0"
        );
        assert!(
            (0.0..=1.0).contains(&critical_threshold),
            "critical_threshold must be between 0.0 and 1.0"
        );
        assert!(
            warning_threshold < critical_threshold,
            "warning_threshold must be less than critical_threshold"
        );

        Self {
            warning_threshold,
            critical_threshold,
        }
    }

    /// Calculate context level using this configuration.
    pub fn check_level(&self, session: &Session) -> ContextLevel {
        if session.context_limit == 0 {
            return ContextLevel::Green;
        }

        let usage = session.context_tokens as f32 / session.context_limit as f32;

        if usage >= self.critical_threshold {
            ContextLevel::Red
        } else if usage >= self.warning_threshold {
            ContextLevel::Yellow
        } else {
            ContextLevel::Green
        }
    }
}

/// Check the context threshold level for a session using default thresholds.
///
/// # Arguments
/// * `session` - The session to check
///
/// # Returns
/// The context level (Green, Yellow, or Red)
///
/// # Examples
/// ```
/// use loom::models::session::Session;
/// use loom::handoff::detector::{check_context_threshold, ContextLevel};
///
/// let mut session = Session::new();
/// session.context_tokens = 50_000;
/// session.context_limit = 200_000;
///
/// assert_eq!(check_context_threshold(&session), ContextLevel::Green);
/// ```
pub fn check_context_threshold(session: &Session) -> ContextLevel {
    ThresholdConfig::default().check_level(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_session(tokens: u32, limit: u32) -> Session {
        let mut session = Session::new();
        session.context_tokens = tokens;
        session.context_limit = limit;
        session
    }

    #[test]
    fn test_context_levels() {
        // Green: below 50% (100k tokens)
        assert_eq!(
            check_context_threshold(&create_test_session(50_000, 200_000)), // 25%
            ContextLevel::Green
        );
        assert_eq!(
            check_context_threshold(&create_test_session(90_000, 200_000)), // 45%
            ContextLevel::Green
        );
        // Yellow: 50-64% (100k-130k tokens)
        assert_eq!(
            check_context_threshold(&create_test_session(100_000, 200_000)), // 50%
            ContextLevel::Yellow
        );
        assert_eq!(
            check_context_threshold(&create_test_session(120_000, 200_000)), // 60%
            ContextLevel::Yellow
        );
        // Red: 65%+ (130k+ tokens)
        assert_eq!(
            check_context_threshold(&create_test_session(130_000, 200_000)), // 65%
            ContextLevel::Red
        );
        assert_eq!(
            check_context_threshold(&create_test_session(160_000, 200_000)), // 80%
            ContextLevel::Red
        );
        // Edge case: zero limit
        assert_eq!(
            check_context_threshold(&create_test_session(100, 0)),
            ContextLevel::Green
        );
    }

    #[test]
    fn test_threshold_config_default() {
        let config = ThresholdConfig::default();
        assert_eq!(config.warning_threshold, 0.50);
        assert_eq!(config.critical_threshold, 0.65);
    }

    #[test]
    fn test_threshold_config_custom() {
        let config = ThresholdConfig::new(0.50, 0.80);
        assert_eq!(config.warning_threshold, 0.50);
        assert_eq!(config.critical_threshold, 0.80);
    }

    #[test]
    #[should_panic(expected = "warning_threshold must be less than critical_threshold")]
    fn test_threshold_config_invalid_order() {
        ThresholdConfig::new(0.80, 0.50);
    }

    #[test]
    #[should_panic(expected = "warning_threshold must be between 0.0 and 1.0")]
    fn test_threshold_config_warning_out_of_range() {
        ThresholdConfig::new(1.5, 0.75);
    }

    #[test]
    #[should_panic(expected = "critical_threshold must be between 0.0 and 1.0")]
    fn test_threshold_config_critical_out_of_range() {
        ThresholdConfig::new(0.60, 1.5);
    }

    #[test]
    fn test_threshold_config_check_level() {
        let config = ThresholdConfig::new(0.50, 0.80);

        let session_low = create_test_session(80_000, 200_000);
        assert_eq!(config.check_level(&session_low), ContextLevel::Green);

        let session_mid = create_test_session(120_000, 200_000);
        assert_eq!(config.check_level(&session_mid), ContextLevel::Yellow);

        let session_high = create_test_session(170_000, 200_000);
        assert_eq!(config.check_level(&session_high), ContextLevel::Red);
    }

    #[test]
    fn test_threshold_config_edge_cases() {
        let config = ThresholdConfig::new(0.50, 0.80);

        let session_exactly_50 = create_test_session(100_000, 200_000);
        assert_eq!(
            config.check_level(&session_exactly_50),
            ContextLevel::Yellow
        );

        let session_exactly_80 = create_test_session(160_000, 200_000);
        assert_eq!(config.check_level(&session_exactly_80), ContextLevel::Red);

        let session_just_below_50 = create_test_session(99_999, 200_000);
        assert_eq!(
            config.check_level(&session_just_below_50),
            ContextLevel::Green
        );

        let session_just_below_80 = create_test_session(159_999, 200_000);
        assert_eq!(
            config.check_level(&session_just_below_80),
            ContextLevel::Yellow
        );
    }

    #[test]
    fn test_edge_cases() {
        // Just below 50% warning threshold
        assert_eq!(
            check_context_threshold(&create_test_session(99_000, 200_000)), // 49.5%
            ContextLevel::Green
        );
        // Just below 65% critical threshold
        assert_eq!(
            check_context_threshold(&create_test_session(129_000, 200_000)), // 64.5%
            ContextLevel::Yellow
        );
        // At max values - should be Red (100% >= 65%)
        assert_eq!(
            check_context_threshold(&create_test_session(u32::MAX, u32::MAX)),
            ContextLevel::Red
        );
        // Zero tokens
        assert_eq!(
            check_context_threshold(&create_test_session(0, 200_000)),
            ContextLevel::Green
        );
    }
}
