//! Clap-level selectors for `loom stage amend`.

use crate::plan::{AmendmentField, AmendmentPatch};
use anyhow::{bail, Result};
use clap::ValueEnum;

/// Which array on a stage `loom stage amend` mutates.
///
/// Mirrors `crate::plan::AmendmentField` one-for-one; kept as a separate,
/// clap-level mirror of `crate::plan::AmendmentField`; [`AmendField::to_field`]
/// maps it across so `cli::dispatch` stays a thin routing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AmendField {
    /// Mutate the `acceptance` array.
    Acceptance,
    /// Mutate the `wiring` array.
    Wiring,
    /// Mutate the `wiring_tests` array.
    WiringTests,
}

/// What to do at `--index` within the field targeted by `loom stage amend`.
///
/// Mirrors `crate::plan::AmendmentPatch`'s variants (minus their payloads),
/// which [`AmendOp::to_patch`] reconstructs from `op` + `index` + `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AmendOp {
    /// Replace the element at `--index` with `--value`.
    Replace,
    /// Insert `--value` at `--index`, shifting existing elements right.
    Insert,
    /// Remove the element at `--index`.
    Delete,
}

impl AmendField {
    /// Map to the plan-level field selector.
    pub fn to_field(self) -> AmendmentField {
        match self {
            AmendField::Acceptance => AmendmentField::Acceptance,
            AmendField::Wiring => AmendmentField::Wiring,
            AmendField::WiringTests => AmendmentField::WiringTests,
        }
    }
}

impl AmendOp {
    /// Build the plan-level patch, enforcing the value/op pairing that clap
    /// cannot express: `replace`/`insert` need `--value`, `delete` refuses it.
    pub fn to_patch(self, index: usize, value: Option<String>) -> Result<AmendmentPatch> {
        match (self, value) {
            (AmendOp::Replace, Some(value)) => Ok(AmendmentPatch::Replace { index, value }),
            (AmendOp::Insert, Some(value)) => Ok(AmendmentPatch::Insert { index, value }),
            (AmendOp::Delete, None) => Ok(AmendmentPatch::Delete { index }),
            (AmendOp::Replace, None) => bail!("--value is required for --op replace"),
            (AmendOp::Insert, None) => bail!("--value is required for --op insert"),
            (AmendOp::Delete, Some(_)) => bail!("--value is not accepted with --op delete"),
        }
    }
}
