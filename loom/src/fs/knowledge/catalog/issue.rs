use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A bounded reason why declared source evidence could not be assessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceUnavailableReason {
    MissingRevision,
    InvalidRevision,
    MissingRepository,
    GitUnavailable,
    CommandFailed,
    ResourceLimit,
}

impl EvidenceUnavailableReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MissingRevision => "missing_revision",
            Self::InvalidRevision => "invalid_revision",
            Self::MissingRepository => "missing_repository",
            Self::GitUnavailable => "git_unavailable",
            Self::CommandFailed => "command_failed",
            Self::ResourceLimit => "resource_limit",
        }
    }
}

/// A problem found in the knowledge base. Reported, never repaired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogIssue {
    DuplicateHeading {
        file: PathBuf,
        heading: String,
        occurrences: usize,
    },
    GenericBlurb {
        file: PathBuf,
        blurb: String,
    },
    BrokenLink {
        file: PathBuf,
        target: String,
    },
    MissingSourceRef {
        file: PathBuf,
        source_path: String,
    },
    EvidenceChanged {
        file: PathBuf,
        source_path: String,
        verified: String,
    },
    EvidenceUnavailable {
        file: PathBuf,
        source_path: String,
        reason: EvidenceUnavailableReason,
    },
    UnverifiableReference {
        file: PathBuf,
        source_path: String,
        kind: String,
    },
    OversizedSection {
        file: PathBuf,
        heading: String,
        lines: usize,
    },
    OversizedFile {
        file: PathBuf,
        lines: usize,
    },
    OversizedIndex {
        bytes: u64,
    },
    /// The same heading in two or more files: a topic kept in several places.
    DuplicateHeadingAcrossFiles {
        heading: String,
        /// Sorted, at least two entries.
        files: Vec<PathBuf>,
    },
}

impl CatalogIssue {
    /// Review prompts and explanatory notes do not make `check --strict` fail.
    pub fn is_review_only(&self) -> bool {
        matches!(
            self,
            Self::EvidenceChanged { .. }
                | Self::EvidenceUnavailable { .. }
                | Self::UnverifiableReference { .. }
                | Self::DuplicateHeadingAcrossFiles { .. }
        )
    }
}
