//! Arguments of `loom stage dispute-findings`, `dispute-contract` and
//! `dispute-integrity` (DESIGN D15): one request each, several ids per request
//! so one retire and respawn covers a whole review round.

use crate::commands::stage::{DisputeFiling, DisputeTarget};
use crate::validation::{clap_description_validator, clap_id_validator};
use clap::Args;

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
