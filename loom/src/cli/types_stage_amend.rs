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
    /// Mutate the `contracts` array.
    Contracts,
    /// Mutate the `harness` array.
    Harness,
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
            AmendField::Contracts => AmendmentField::Contracts,
            AmendField::Harness => AmendmentField::Harness,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::types::{Cli, Commands};
    use crate::cli::types_stage::StageCommands;
    use clap::Parser;

    #[test]
    fn harness_field_parses_to_the_plan_field() {
        let argv = [
            "loom",
            "stage",
            "amend",
            "s1",
            "--field",
            "harness",
            "--op",
            "insert",
            "--index",
            "0",
            "--value",
            "tests/fixtures/**",
        ];
        let cli = Cli::try_parse_from(argv).unwrap();
        let Commands::Stage {
            command: StageCommands::Amend { field, .. },
        } = cli.command
        else {
            panic!("amend parsed as another command");
        };
        assert_eq!(field.to_field(), AmendmentField::Harness);
    }
}
