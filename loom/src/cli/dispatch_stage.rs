//! `loom stage <subcommand>` dispatch.
//!
//! Routes stage completion, admin proof, transitions, and adjudication.

use anyhow::Result;

use super::dispatch::{print_minted_proof, resolve_completion_proof};
use super::types::{OutputCommands, StageCommands};
use super::types_stage::{ContractsCommands, CriteriaDispute, ReviewCommands};
use crate::commands::stage;
use crate::commands::stage::commit;

/// `loom stage {block,reset,waiting,resume,hold,release,skip}` dispatch.
fn dispatch_stage_transitions(command: StageCommands) -> Result<()> {
    match command {
        StageCommands::Block { stage_id, reason } => stage::block(stage_id, reason),
        StageCommands::Reset {
            stage_id,
            hard,
            kill_session,
        } => stage::reset(stage_id, hard, kill_session),
        StageCommands::Waiting { stage_id } => stage::waiting(stage_id),
        StageCommands::Resume { stage_id } => stage::resume_from_waiting(stage_id),
        StageCommands::Hold { stage_id } => stage::hold(stage_id),
        StageCommands::Release { stage_id } => stage::release(stage_id),
        StageCommands::Skip { stage_id, reason } => stage::skip(stage_id, reason),
        _ => unreachable!("dispatch_stage routes only the transition group here"),
    }
}

/// `loom stage {dispute-criteria,adjudicate,amend}` dispatch.
fn dispatch_stage_criteria(command: StageCommands) -> Result<()> {
    match command {
        StageCommands::DisputeCriteria(args) => match CriteriaDispute::try_from(args)? {
            CriteriaDispute::Entry {
                stage_id,
                field,
                criterion_indices,
                reason,
                evidence_commit,
                failure_output,
            } => stage::dispute_criteria(
                stage_id,
                field,
                criterion_indices,
                reason,
                evidence_commit,
                failure_output,
            ),
            CriteriaDispute::ImpactTests(filing) => stage::file_dispute(filing),
        },
        StageCommands::Adjudicate {
            stage,
            dispute,
            verdict_file,
        } => stage::adjudicate(stage, dispute, verdict_file),
        StageCommands::Amend {
            stage_id,
            field,
            op,
            index,
            value,
            reason,
        } => stage::amend(
            stage_id,
            field.to_field(),
            op.to_patch(index, value)?,
            reason,
        ),
        _ => unreachable!("dispatch_stage routes only the criteria-adjudication group here"),
    }
}

/// Resolve the completion proof before completing the stage.
fn dispatch_complete(command: StageCommands) -> Result<()> {
    let StageCommands::Complete {
        stage_id,
        session,
        no_verify,
        force_unsafe,
        assume_merged,
        no_cache,
    } = command
    else {
        unreachable!("dispatch_stage routes only Complete here");
    };
    let admin_proof = resolve_completion_proof(&stage_id, no_verify, force_unsafe, assume_merged)?;
    stage::complete(
        stage_id,
        session,
        no_verify,
        force_unsafe,
        assume_merged,
        no_cache,
        admin_proof,
    )
}

/// Join the repeated `-m` values of `loom stage commit` into one message,
/// separating paragraphs with a blank line as `git commit` does.
fn join_commit_message(parts: &[String]) -> String {
    parts.join("\n\n")
}

/// `loom stage <subcommand>` dispatch.
pub(super) fn dispatch_stage(command: StageCommands) -> Result<()> {
    match command {
        cmd @ StageCommands::Complete { .. } => dispatch_complete(cmd),
        StageCommands::AdminProof {
            stage_id,
            daemon_stop,
            no_verify,
            force_unsafe,
            assume_merged,
        } => print_minted_proof(
            stage_id,
            daemon_stop,
            no_verify,
            force_unsafe,
            assume_merged,
        ),
        cmd @ (StageCommands::Block { .. }
        | StageCommands::Reset { .. }
        | StageCommands::Waiting { .. }
        | StageCommands::Resume { .. }
        | StageCommands::Hold { .. }
        | StageCommands::Release { .. }
        | StageCommands::Skip { .. }) => dispatch_stage_transitions(cmd),
        StageCommands::Retry {
            stage_id,
            force,
            context,
        } => stage::retry(stage_id, force, context),
        StageCommands::Merge { stage_id, resolved } => stage::merge(stage_id, resolved),
        StageCommands::Commit { stage_id, message } => {
            commit::execute(stage_id, join_commit_message(&message))
        }
        StageCommands::HumanReview {
            stage_id,
            approve,
            force_complete,
            reject,
        } => stage::human_review(stage_id, approve, force_complete, reject),
        cmd @ (StageCommands::DisputeCriteria(_)
        | StageCommands::Adjudicate { .. }
        | StageCommands::Amend { .. }) => dispatch_stage_criteria(cmd),
        StageCommands::DisputeFindings(args) => stage::file_dispute(args.into()),
        StageCommands::DisputeContract(args) => stage::file_dispute(args.into()),
        StageCommands::DisputeIntegrity(args) => stage::file_dispute(args.into()),
        StageCommands::Output { command } => dispatch_stage_output(command),
        StageCommands::Contracts { command } => dispatch_stage_contracts(command),
        StageCommands::Review { command } => dispatch_stage_review(command),
        StageCommands::VerifyHost { stage_id, job } => stage::verify_host::execute(stage_id, job),
    }
}

/// `loom stage output <subcommand>` dispatch, broken out of `dispatch_stage`
/// to pay for the `contracts` arm without growing the parent match.
fn dispatch_stage_output(command: OutputCommands) -> Result<()> {
    match command {
        OutputCommands::Set {
            stage_id,
            key,
            value,
            description,
        } => stage::output_set(stage_id, key, value, description),
        OutputCommands::Get { stage_id, key } => stage::output_get(stage_id, key),
        OutputCommands::List { stage_id } => stage::output_list(stage_id),
        OutputCommands::Remove { stage_id, key } => stage::output_remove(stage_id, key),
    }
}

/// `loom stage contracts <subcommand>` dispatch.
fn dispatch_stage_contracts(command: ContractsCommands) -> Result<()> {
    match command {
        ContractsCommands::Freeze { stage_id } => stage::contracts_freeze(stage_id),
        ContractsCommands::Show { stage_id } => stage::contracts_show(stage_id),
        ContractsCommands::Restore { stage_id, contract } => {
            stage::contracts_restore(stage_id, contract)
        }
    }
}

/// `loom stage review <subcommand>` dispatch.
fn dispatch_stage_review(command: ReviewCommands) -> Result<()> {
    match command {
        ReviewCommands::Status { stage_id } => stage::review_status(stage_id),
        ReviewCommands::Integrity { stage_id } => stage::review_integrity(stage_id),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::types::{Cli, Commands};

    fn commit_parts(args: &[&str]) -> Result<Vec<String>, String> {
        let mut argv = vec!["loom", "stage", "commit", "stage-a"];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv).map_err(|error| error.to_string())?;
        match cli.command {
            Commands::Stage {
                command: StageCommands::Commit { message, .. },
            } => Ok(message),
            _ => panic!("expected StageCommands::Commit"),
        }
    }

    #[test]
    fn repeated_message_flags_join_with_a_blank_line() {
        let parts = commit_parts(&["-m", "feat(a): subject", "-m", "body paragraph"])
            .expect("repeated -m parses");
        assert_eq!(
            join_commit_message(&parts),
            "feat(a): subject\n\nbody paragraph"
        );
    }

    #[test]
    fn single_message_is_unchanged_and_long_form_parses() {
        let parts = commit_parts(&["--message", "fix(a): only"]).expect("--message parses");
        assert_eq!(join_commit_message(&parts), "fix(a): only");
    }

    #[test]
    fn message_is_required() {
        assert!(commit_parts(&[]).is_err());
    }
}
