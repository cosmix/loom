use serde::{Deserialize, Serialize};

/// Status of goal-backward verification for a stage
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    /// Verification has not been run
    #[default]
    NotRun,
    /// All verifications passed
    Passed,
    /// Gaps were found
    GapsFound {
        /// Number of gaps found
        gap_count: usize,
    },
    /// Some checks require human judgment
    HumanNeeded,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationStatus::NotRun => write!(f, "NotRun"),
            VerificationStatus::Passed => write!(f, "Passed"),
            VerificationStatus::GapsFound { gap_count } => write!(f, "GapsFound({gap_count})"),
            VerificationStatus::HumanNeeded => write!(f, "HumanNeeded"),
        }
    }
}
