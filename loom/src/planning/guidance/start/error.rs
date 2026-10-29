//! Typed refusals for authoring starts.

use std::fmt;

/// Why an action cannot start an authoring session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// Team authoring is implemented by the later kickoff-session stage.
    NotAvailableYet(String),
    /// Moving a document is a file operation, not an authoring session.
    FileMove(String),
    /// Pressure and run actions have separate session flows.
    NotAuthoring(String),
    /// The selected feature does not currently offer this action.
    NotOffered(String),
    /// Feature authoring requires an explicit feature in the session request.
    FeatureRequired,
    /// The selected feature differs from the action's owner.
    FeatureMismatch(String),
    /// A feature name or standalone plan slug cannot form a safe target path.
    InvalidTarget(String),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAvailableYet(id) => write!(f, "{id} is not available yet"),
            Self::FileMove(id) => write!(f, "{id} is a file move, not a session"),
            Self::NotAuthoring(id) => write!(f, "{id} is not an authoring offer"),
            Self::NotOffered(id) => write!(f, "the step \"{id}\" is not offered now"),
            Self::FeatureRequired => f.write_str("an authoring offer requires a feature"),
            Self::FeatureMismatch(id) => write!(f, "{id} does not belong to the requested feature"),
            Self::InvalidTarget(name) => write!(f, "invalid authoring target: {name}"),
        }
    }
}

impl std::error::Error for StartError {}
