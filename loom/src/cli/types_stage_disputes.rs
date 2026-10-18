//! Arguments of `loom stage dispute-criteria`, `dispute-findings`,
//! `dispute-contract` and `dispute-integrity` (DESIGN D15): one request each,
//! several ids per request so one retire and respawn covers a whole review
//! round.

use crate::commands::stage::{DisputeFiling, DisputeTarget};
use crate::models::dispute::CriterionField;
use crate::validation::{clap_description_validator, clap_id_validator};
use crate::verify::impact_tests::check_failure_id;
use anyhow::{bail, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// What `loom stage dispute-criteria` disputes.
///
/// A clap-level mirror of [`CriterionField`], the pattern `AmendField` uses,
/// plus `impact-tests`, which names failing tests rather than a plan list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DisputeField {
    /// The stage's acceptance criteria.
    Acceptance,
    /// The stage's wiring checks.
    Wiring,
    /// The stage's wiring tests.
    WiringTests,
    /// Failing impact-selected tests the stage did not cause.
    ImpactTests,
}

impl DisputeField {
    /// The model-level list selector; `None` for `impact-tests`.
    pub fn to_field(self) -> Option<CriterionField> {
        match self {
            DisputeField::Acceptance => Some(CriterionField::Acceptance),
            DisputeField::Wiring => Some(CriterionField::Wiring),
            DisputeField::WiringTests => Some(CriterionField::WiringTests),
            DisputeField::ImpactTests => None,
        }
    }
}

#[derive(Args)]
pub struct DisputeCriteriaArgs {
    /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
    #[arg(value_parser = clap_id_validator)]
    pub stage_id: String,

    /// Which list --criterion-index indexes: `acceptance` (the default),
    /// `wiring` or `wiring-tests`, as `loom stage complete` labels a failure
    /// `[criterion n]`, `[wiring n]` or `[wiring_tests n]`. `impact-tests`
    /// disputes failing impact-selected tests, named with --test.
    #[arg(long, value_enum, default_value = "acceptance")]
    pub field: DisputeField,

    /// Index (0-based) of the disputed entry in --field's list.
    #[arg(long = "criterion-index")]
    pub criterion_index: Option<usize>,

    /// With --field impact-tests: the id of a failing impact-selected test
    /// selection (`IT-<runner>:<package>`), as `loom stage complete` lists it.
    #[arg(long = "test", num_args = 1..)]
    pub tests: Vec<String>,

    /// Reason the criterion is wrong or impossible, or why the stage did not
    /// cause the failing tests (max 500 chars).
    #[arg(long, value_parser = clap_description_validator)]
    pub reason: String,

    /// Optional commit SHA cited as evidence.
    #[arg(long = "evidence-commit")]
    pub evidence_commit: Option<String>,

    /// Optional path to a captured failure-output file (truncated to 4KB).
    /// An impact-test dispute takes none: loom recorded each test's output.
    #[arg(long = "failure-output")]
    pub failure_output: Option<PathBuf>,
}

/// What one `loom stage dispute-criteria` invocation files.
pub enum CriteriaDispute {
    /// One entry of a stage's acceptance, wiring or wiring-tests list.
    Entry {
        stage_id: String,
        field: CriterionField,
        criterion_index: usize,
        reason: String,
        evidence_commit: Option<String>,
        failure_output: Option<PathBuf>,
    },
    /// Failing impact-selected tests.
    ImpactTests(DisputeFiling),
}

impl TryFrom<DisputeCriteriaArgs> for CriteriaDispute {
    type Error = anyhow::Error;

    fn try_from(args: DisputeCriteriaArgs) -> Result<Self> {
        let Some(field) = args.field.to_field() else {
            if args.criterion_index.is_some() || args.failure_output.is_some() {
                bail!(
                    "--field impact-tests names the failing tests with --test; it takes no \
                     --criterion-index or --failure-output"
                );
            }
            if args.tests.is_empty() {
                bail!("--field impact-tests needs --test <id> for each failing selection");
            }
            for id in &args.tests {
                check_failure_id(id)?;
            }
            return Ok(CriteriaDispute::ImpactTests(DisputeFiling {
                stage_id: args.stage_id,
                target: DisputeTarget::ImpactTests(args.tests),
                reason: args.reason,
                evidence_commit: args.evidence_commit,
            }));
        };
        if !args.tests.is_empty() {
            bail!("--test names failing impact-selected tests; use it with --field impact-tests");
        }
        let Some(criterion_index) = args.criterion_index else {
            bail!("--field {} needs --criterion-index", field.as_str());
        };
        Ok(CriteriaDispute::Entry {
            stage_id: args.stage_id,
            field,
            criterion_index,
            reason: args.reason,
            evidence_commit: args.evidence_commit,
            failure_output: args.failure_output,
        })
    }
}

#[derive(Args)]
pub struct DisputeFindingsArgs {
    /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
    #[arg(value_parser = clap_id_validator)]
    pub stage_id: String,

    /// Id of an open finding (`F-<round>-<n>`, or `<stage>/F-<round>-<n>` when
    /// carried), as `loom stage review status` lists it.
    #[arg(long = "finding", required = true, num_args = 1..)]
    pub findings: Vec<String>,

    /// Why the findings are wrong (max 500 chars).
    #[arg(long, value_parser = clap_description_validator)]
    pub reason: String,

    /// Optional commit SHA cited as evidence.
    #[arg(long = "evidence-commit")]
    pub evidence_commit: Option<String>,
}

#[derive(Args)]
pub struct DisputeContractArgs {
    /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
    #[arg(value_parser = clap_id_validator)]
    pub stage_id: String,

    /// Id of the frozen contract, as `loom stage contracts show` lists it.
    #[arg(long)]
    pub contract: String,

    /// Why the contract is wrong (max 500 chars).
    #[arg(long, value_parser = clap_description_validator)]
    pub reason: String,

    /// Optional commit SHA cited as evidence.
    #[arg(long = "evidence-commit")]
    pub evidence_commit: Option<String>,
}

#[derive(Args)]
pub struct DisputeIntegrityArgs {
    /// Stage ID (alphanumeric, dash, underscore only; max 128 characters)
    #[arg(value_parser = clap_id_validator)]
    pub stage_id: String,

    /// Id of a current test-integrity event, as `loom stage review integrity`
    /// lists it.
    #[arg(long = "event", required = true, num_args = 1..)]
    pub events: Vec<String>,

    /// Why the change behind the events is sound (max 500 chars).
    #[arg(long, value_parser = clap_description_validator)]
    pub reason: String,

    /// Optional commit SHA cited as evidence.
    #[arg(long = "evidence-commit")]
    pub evidence_commit: Option<String>,
}

impl From<DisputeFindingsArgs> for DisputeFiling {
    fn from(args: DisputeFindingsArgs) -> Self {
        Self {
            stage_id: args.stage_id,
            target: DisputeTarget::Findings(args.findings),
            reason: args.reason,
            evidence_commit: args.evidence_commit,
        }
    }
}

impl From<DisputeContractArgs> for DisputeFiling {
    fn from(args: DisputeContractArgs) -> Self {
        Self {
            stage_id: args.stage_id,
            target: DisputeTarget::Contract(args.contract),
            reason: args.reason,
            evidence_commit: args.evidence_commit,
        }
    }
}

impl From<DisputeIntegrityArgs> for DisputeFiling {
    fn from(args: DisputeIntegrityArgs) -> Self {
        Self {
            stage_id: args.stage_id,
            target: DisputeTarget::Integrity(args.events),
            reason: args.reason,
            evidence_commit: args.evidence_commit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::types::{Cli, Commands};
    use crate::cli::types_stage::StageCommands;
    use clap::Parser;

    /// What `loom stage dispute-criteria <args>` files, or why it refuses.
    fn parsed(args: &[&str]) -> Result<CriteriaDispute> {
        let argv = ["loom", "stage", "dispute-criteria", "s1", "--reason", "r"];
        let cli = Cli::try_parse_from(argv.iter().chain(args))?;
        let Commands::Stage {
            command: StageCommands::DisputeCriteria(args),
        } = cli.command
        else {
            panic!("dispute-criteria parsed as another command");
        };
        CriteriaDispute::try_from(args)
    }

    #[test]
    fn impact_tests_file_a_plan_v2_dispute_naming_each_test() {
        let ids = ["IT-cargo-test:.", "IT-pytest:deps/kit-a"];
        let dispute = parsed(&["--field", "impact-tests", "--test", ids[0], ids[1]]).unwrap();

        let CriteriaDispute::ImpactTests(filing) = dispute else {
            panic!("expected an impact-test filing");
        };
        let tests = ids.map(str::to_string).to_vec();
        assert_eq!(filing.target, DisputeTarget::ImpactTests(tests));
        assert_eq!(filing.stage_id, "s1");
    }

    #[test]
    fn a_list_entry_still_needs_its_index() {
        let entry = parsed(&["--field", "wiring", "--criterion-index", "2"]).unwrap();
        assert!(matches!(
            entry,
            CriteriaDispute::Entry {
                field: CriterionField::Wiring,
                criterion_index: 2,
                ..
            }
        ));
        let missing = parsed(&["--field", "wiring"]).err().unwrap();
        assert!(missing.to_string().contains("needs --criterion-index"));
    }

    #[test]
    fn tests_and_indexes_do_not_mix() {
        for (args, expected) in [
            (&["--field", "impact-tests"][..], "needs --test"),
            (
                &["--field", "impact-tests", "--test", "IT-cargo-test-."],
                "is not an impact-test id",
            ),
            (
                &["--field", "impact-tests", "--test", "IT-no-such-runner:."],
                "names no test runner",
            ),
            (
                &[
                    "--field",
                    "impact-tests",
                    "--test",
                    "IT-cargo-test:.",
                    "--criterion-index",
                    "0",
                ],
                "takes no --criterion-index",
            ),
            (
                &["--criterion-index", "0", "--test", "IT-cargo-test:."],
                "use it with --field impact-tests",
            ),
        ] {
            let error = parsed(args).err().unwrap();
            assert!(error.to_string().contains(expected), "{args:?}: {error}");
        }
    }
}
