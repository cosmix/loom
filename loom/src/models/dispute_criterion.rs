//! What a criterion dispute names: the list it indexes and, for an
//! integration-verify stage's aggregated wiring re-verification, the
//! completed stage that owns that list.

use serde::{Deserialize, Serialize};

use crate::plan::amendment::AmendmentField;

/// Which list of a stage a criterion dispute contests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CriterionField {
    /// `acceptance`: the stage's acceptance criteria.
    #[default]
    Acceptance,
    /// `wiring`: the stage's wiring checks.
    Wiring,
    /// `wiring_tests`: the stage's wiring tests.
    WiringTests,
    /// `artifacts`: the stage's artifact patterns.
    Artifacts,
}

impl CriterionField {
    /// The CLI and serde spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriterionField::Acceptance => "acceptance",
            CriterionField::Wiring => "wiring",
            CriterionField::WiringTests => "wiring-tests",
            CriterionField::Artifacts => "artifacts",
        }
    }

    /// The label `loom stage complete` prints as `[<label> n]`.
    pub fn gap_label(self) -> &'static str {
        match self {
            CriterionField::Acceptance => "criterion",
            CriterionField::Wiring => "wiring",
            CriterionField::WiringTests => "wiring_tests",
            CriterionField::Artifacts => "artifact",
        }
    }

    /// The plan-amendment field that mutates this list.
    pub fn amendment_field(self) -> AmendmentField {
        match self {
            CriterionField::Acceptance => AmendmentField::Acceptance,
            CriterionField::Wiring => AmendmentField::Wiring,
            CriterionField::WiringTests => AmendmentField::WiringTests,
            CriterionField::Artifacts => AmendmentField::Artifacts,
        }
    }
}

/// The entries one `loom stage dispute-criteria` filing names: indices into
/// `field`'s list of `owner_stage`, or of the filing stage when that is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionTarget {
    pub field: CriterionField,
    pub criterion_indices: Vec<usize>,
    pub owner_stage: Option<String>,
}

impl CriterionTarget {
    pub fn new(
        field: CriterionField,
        criterion_indices: Vec<usize>,
        owner_stage: Option<String>,
    ) -> Self {
        Self {
            field,
            criterion_indices,
            owner_stage,
        }
    }

    /// Entries of the filing stage's own `field` list.
    pub fn own(field: CriterionField, criterion_indices: Vec<usize>) -> Self {
        Self {
            field,
            criterion_indices,
            owner_stage: None,
        }
    }
}
