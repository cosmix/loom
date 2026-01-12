use crate::models::failure::FailureType;
use chrono::{DateTime, Utc};
use std::time::Duration;

/// Determines if a stage failure should trigger an automatic retry.
///
/// Only transient failures (SessionCrash, Timeout) are eligible for auto-retry,
/// and only if the retry count hasn't exceeded the maximum.
///
/// ContextExhausted uses the handoff mechanism instead of retry.
/// Code issues (TestFailure, BuildFailure, CodeError) require diagnosis.
pub fn should_auto_retry(failure_type: &FailureType, retry_count: u32, max_retries: u32) -> bool {
    if retry_count >= max_retries {
        return false;
    }

    matches!(
        failure_type,
        FailureType::SessionCrash | FailureType::Timeout
    )
}

/// Calculates exponential backoff duration for retry attempts.
///
/// Formula: base_secs * 2^(retry_count-1), capped at max_secs
///
/// # Examples
///
/// With base_secs=30, max_secs=300:
/// - Retry 1: 30s
/// - Retry 2: 60s
/// - Retry 3: 120s
/// - Retry 4: 240s
/// - Retry 5+: 300s (capped)
pub fn calculate_backoff(retry_count: u32, base_secs: u64, max_secs: u64) -> Duration {
    if retry_count == 0 {
        return Duration::from_secs(0);
    }

    // Calculate 2^(retry_count - 1)
    let exponent = retry_count - 1;
    let multiplier = 2u64.saturating_pow(exponent);

    // Calculate backoff_secs = base_secs * multiplier, capped at max_secs
    let backoff_secs = base_secs.saturating_mul(multiplier).min(max_secs);

    Duration::from_secs(backoff_secs)
}

/// Checks if enough time has elapsed since the last failure to allow a retry.
///
/// Returns true if:
/// - last_failure is None (no previous failure), or
/// - The time elapsed since last_failure is >= backoff duration
pub fn is_backoff_elapsed(last_failure: Option<DateTime<Utc>>, backoff: Duration) -> bool {
    match last_failure {
        None => true,
        Some(last_time) => {
            let now = Utc::now();
            let elapsed = now.signed_duration_since(last_time);

            // Convert backoff to chrono::Duration for comparison
            if let Ok(backoff_chrono) = chrono::Duration::from_std(backoff) {
                elapsed >= backoff_chrono
            } else {
                // If conversion fails (extremely large duration), consider backoff not elapsed
                false
            }
        }
    }
}

/// Classifies a failure based on the close reason string.
///
/// Analyzes the close_reason text to determine what type of failure occurred.
/// This enables appropriate handling strategies (retry, handoff, diagnosis, etc.).
pub fn classify_failure(close_reason: &str) -> FailureType {
    let reason_lower = close_reason.to_lowercase();

    // SessionCrash: Process-level failures
    if reason_lower.contains("crash")
        || reason_lower.contains("process")
        || reason_lower.contains("orphan")
    {
        return FailureType::SessionCrash;
    }

    // ContextExhausted: Token/context limit reached
    if reason_lower.contains("context")
        || reason_lower.contains("token")
        || reason_lower.contains("handoff")
    {
        return FailureType::ContextExhausted;
    }

    // BuildFailure: Compilation/build failures (check before TestFailure due to "failed" keyword)
    if reason_lower.contains("build")
        || reason_lower.contains("compilation")
        || reason_lower.contains("rustc")
        || reason_lower.contains("tsc")
    {
        return FailureType::BuildFailure;
    }

    // TestFailure: Test execution failures
    if reason_lower.contains("test")
        || reason_lower.contains("assertion")
        || reason_lower.contains("failed")
    {
        return FailureType::TestFailure;
    }

    // CodeError: Lint and type errors
    if reason_lower.contains("lint")
        || reason_lower.contains("type error")
        || reason_lower.contains("syntax")
    {
        return FailureType::CodeError;
    }

    // Timeout: Execution timeout
    if reason_lower.contains("timeout") {
        return FailureType::Timeout;
    }

    // UserBlocked: Manually blocked by user
    if reason_lower.contains("blocked by user") || reason_lower.contains("manually blocked") {
        return FailureType::UserBlocked;
    }

    // MergeConflict: Git merge conflicts
    if reason_lower.contains("conflict") || reason_lower.contains("merge") {
        return FailureType::MergeConflict;
    }

    // Unknown: Unclassified failure
    FailureType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_auto_retry() {
        // SessionCrash should retry within limit
        assert!(should_auto_retry(&FailureType::SessionCrash, 0, 3));
        assert!(should_auto_retry(&FailureType::SessionCrash, 2, 3));
        assert!(!should_auto_retry(&FailureType::SessionCrash, 3, 3));

        // Timeout should retry within limit
        assert!(should_auto_retry(&FailureType::Timeout, 0, 3));
        assert!(should_auto_retry(&FailureType::Timeout, 2, 3));
        assert!(!should_auto_retry(&FailureType::Timeout, 3, 3));

        // ContextExhausted should never retry
        assert!(!should_auto_retry(&FailureType::ContextExhausted, 0, 3));

        // Code issues should never retry
        assert!(!should_auto_retry(&FailureType::TestFailure, 0, 3));
        assert!(!should_auto_retry(&FailureType::BuildFailure, 0, 3));
        assert!(!should_auto_retry(&FailureType::CodeError, 0, 3));

        // Other types should never retry
        assert!(!should_auto_retry(&FailureType::UserBlocked, 0, 3));
        assert!(!should_auto_retry(&FailureType::MergeConflict, 0, 3));
        assert!(!should_auto_retry(&FailureType::Unknown, 0, 3));
    }

    #[test]
    fn test_calculate_backoff() {
        // retry_count=0 should return 0
        assert_eq!(calculate_backoff(0, 30, 300), Duration::from_secs(0));

        // Exponential backoff: base=30, max=300
        assert_eq!(calculate_backoff(1, 30, 300), Duration::from_secs(30));
        assert_eq!(calculate_backoff(2, 30, 300), Duration::from_secs(60));
        assert_eq!(calculate_backoff(3, 30, 300), Duration::from_secs(120));
        assert_eq!(calculate_backoff(4, 30, 300), Duration::from_secs(240));
        assert_eq!(calculate_backoff(5, 30, 300), Duration::from_secs(300)); // capped
        assert_eq!(calculate_backoff(6, 30, 300), Duration::from_secs(300)); // still capped

        // Different base and max
        assert_eq!(calculate_backoff(1, 10, 100), Duration::from_secs(10));
        assert_eq!(calculate_backoff(2, 10, 100), Duration::from_secs(20));
        assert_eq!(calculate_backoff(3, 10, 100), Duration::from_secs(40));
        assert_eq!(calculate_backoff(4, 10, 100), Duration::from_secs(80));
        assert_eq!(calculate_backoff(5, 10, 100), Duration::from_secs(100)); // capped
    }

    #[test]
    fn test_is_backoff_elapsed() {
        // None should always return true
        assert!(is_backoff_elapsed(None, Duration::from_secs(30)));

        // Recent failure should not allow retry
        let recent = Utc::now() - chrono::Duration::seconds(10);
        assert!(!is_backoff_elapsed(Some(recent), Duration::from_secs(30)));

        // Old failure should allow retry
        let old = Utc::now() - chrono::Duration::seconds(60);
        assert!(is_backoff_elapsed(Some(old), Duration::from_secs(30)));

        // Exact boundary (edge case)
        let exactly_30_secs_ago = Utc::now() - chrono::Duration::seconds(30);
        assert!(is_backoff_elapsed(
            Some(exactly_30_secs_ago),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn test_classify_failure() {
        // SessionCrash
        assert_eq!(
            classify_failure("Process terminated unexpectedly"),
            FailureType::SessionCrash
        );
        assert_eq!(
            classify_failure("Orphaned session detected"),
            FailureType::SessionCrash
        );

        // ContextExhausted
        assert_eq!(
            classify_failure("Context limit exceeded"),
            FailureType::ContextExhausted
        );
        assert_eq!(
            classify_failure("Token budget exhausted, initiating handoff"),
            FailureType::ContextExhausted
        );

        // TestFailure
        assert_eq!(
            classify_failure("Test assertion failed"),
            FailureType::TestFailure
        );
        assert_eq!(
            classify_failure("23 tests FAILED"),
            FailureType::TestFailure
        );

        // BuildFailure
        assert_eq!(
            classify_failure("Build failed: rustc error"),
            FailureType::BuildFailure
        );
        assert_eq!(
            classify_failure("Compilation error in main.ts (tsc)"),
            FailureType::BuildFailure
        );

        // CodeError
        assert_eq!(
            classify_failure("Lint errors detected"),
            FailureType::CodeError
        );
        assert_eq!(
            classify_failure("Type error: undefined"),
            FailureType::CodeError
        );
        assert_eq!(classify_failure("Syntax error"), FailureType::CodeError);

        // Timeout
        assert_eq!(
            classify_failure("Stage execution timeout"),
            FailureType::Timeout
        );

        // UserBlocked
        assert_eq!(
            classify_failure("Blocked by user for review"),
            FailureType::UserBlocked
        );
        assert_eq!(
            classify_failure("Manually blocked pending approval"),
            FailureType::UserBlocked
        );

        // MergeConflict
        assert_eq!(
            classify_failure("Merge conflict in src/main.rs"),
            FailureType::MergeConflict
        );
        assert_eq!(
            classify_failure("Conflict detected during auto-merge"),
            FailureType::MergeConflict
        );

        // Unknown
        assert_eq!(
            classify_failure("Something went wrong"),
            FailureType::Unknown
        );
        assert_eq!(classify_failure(""), FailureType::Unknown);
    }

    #[test]
    fn test_classify_failure_case_insensitive() {
        assert_eq!(
            classify_failure("Context LIMIT exceeded"),
            FailureType::ContextExhausted
        );
        assert_eq!(classify_failure("BUILD FAILED"), FailureType::BuildFailure);
    }
}
