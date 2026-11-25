//! Contract `artifacts-cli-fields-parse`: `loom stage amend` and
//! `loom stage dispute-criteria` accept `--field artifacts` and map it to the
//! artifacts list.
use crate::cli::types::{Cli, Commands};
use crate::cli::types_stage::{CriteriaDispute, StageCommands};
use crate::models::dispute::CriterionField;
use crate::plan::AmendmentField;
use clap::Parser;

fn stage_command(args: &[&str]) -> StageCommands {
    let argv = ["loom", "stage"].into_iter().chain(args.iter().copied());
    let cli = Cli::try_parse_from(argv).expect("the stage command parses");
    let Commands::Stage { command } = cli.command else {
        panic!("parsed as another command");
    };
    command
}

#[test]
fn artifacts_cli_fields_parse() {
    let amend = ["amend", "s1", "--field", "artifacts", "--op", "replace"];
    let value = ["--index", "0", "--value", "docs/*.md", "--reason", "r"];
    let StageCommands::Amend(args) = stage_command(&[&amend[..], &value[..]].concat()) else {
        panic!("amend parsed as another command");
    };
    assert_eq!(args.field.to_field(), AmendmentField::Artifacts);

    let dispute = ["dispute-criteria", "s1", "--reason", "r"];
    let field = ["--field", "artifacts", "--criterion-index", "1"];
    let StageCommands::DisputeCriteria(args) = stage_command(&[&dispute[..], &field[..]].concat())
    else {
        panic!("dispute-criteria parsed as another command");
    };
    let CriteriaDispute::Entry { target, .. } =
        CriteriaDispute::try_from(args).expect("an artifacts dispute files")
    else {
        panic!("expected a list-entry dispute");
    };
    assert_eq!(target.field, CriterionField::Artifacts);
    assert_eq!(target.criterion_indices, vec![1]);
}
