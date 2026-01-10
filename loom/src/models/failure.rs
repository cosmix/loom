use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of failure that occurred during stage execution.
///
/// Different failure types have different handling strategies:
/// - Transient failures (SessionCrash, Timeout) may be eligible for auto-retry
/// - Code issues (TestFailure, BuildFailure, CodeError) require diagnosis
/// - Structural failures (ContextExhausted, MergeConflict) have specialized handlers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailureType {
    /// Session crashed unexpectedly (transient, auto-retry eligible)
    SessionCrash,

    /// Session ran out of context tokens (handled by handoff mechanism, not retry)
    ContextExhausted,

    /// Test execution failed (code issue, needs diagnosis)
    TestFailure,

    /// Build/compilation failed (code issue, needs diagnosis)
    BuildFailure,

    /// Code execution error (code issue, needs diagnosis)
    CodeError,

    /// Stage execution timed out (possibly transient)
    Timeout,

    /// User explicitly blocked the stage
    UserBlocked,

    /// Merge conflict occurred (handled by dedicated merge session)
    MergeConflict,

    /// Unknown or unclassified failure
    Unknown,
}

/// Information about a failure that occurred during stage execution.
///
/// This struct captures the type of failure, when it was detected,
/// and evidence that can be used for diagnosis or retry decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailureInfo {
    /// The type of failure that occurred
    pub failure_type: FailureType,

    /// When the failure was detected
    pub detected_at: DateTime<Utc>,

    /// Evidence of the failure (log excerpts, error messages, etc.)
    pub evidence: Vec<String>,
}
