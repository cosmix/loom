//! Result types for goal-backward verification

use serde::{Deserialize, Serialize};

/// Type of verification gap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GapType {
    /// A truth command returned non-zero exit code
    TruthFailed,
    /// A required artifact file is missing
    ArtifactMissing,
    /// An artifact exists but appears to be a stub
    ArtifactStub,
    /// An artifact exists but is empty
    ArtifactEmpty,
    /// A wiring pattern was not found in source file
    WiringBroken,
    /// Dead code detected in output
    DeadCodeFound,
}

/// A gap between expected and actual verification state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationGap {
    /// Type of gap
    pub gap_type: GapType,
    /// Human-readable description of the gap
    pub description: String,
    /// Suggested fix for the gap
    pub suggestion: String,
}

impl VerificationGap {
    /// Create a new verification gap
    pub fn new(
        gap_type: GapType,
        description: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            gap_type,
            description: description.into(),
            suggestion: suggestion.into(),
        }
    }
}

/// Result of goal-backward verification
#[derive(Debug)]
pub enum GoalBackwardResult {
    /// All verifications passed
    Passed,
    /// Gaps found that may be automatically fixable
    GapsFound { gaps: Vec<VerificationGap> },
    /// Some checks require human judgment
    HumanNeeded { checks: Vec<String> },
}

impl GoalBackwardResult {
    /// Create from a list of gaps
    pub fn from_gaps(gaps: Vec<VerificationGap>) -> Self {
        if gaps.is_empty() {
            Self::Passed
        } else {
            Self::GapsFound { gaps }
        }
    }

    /// Check if all verifications passed
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Get gaps if any
    pub fn gaps(&self) -> &[VerificationGap] {
        match self {
            Self::GapsFound { gaps } => gaps,
            _ => &[],
        }
    }
}
