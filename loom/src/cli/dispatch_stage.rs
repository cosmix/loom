//! `loom stage <subcommand>` dispatch.
//!
//! Split out of `dispatch.rs` because the stage command group — completion,
//! admin-proof minting, the transition lifecycle, and the
//! criteria-adjudication lifecycle — is loom's largest dispatch surface, and
//! it was crowding every other command group toward `dispatch.rs`'s own line
//! ceiling. Giving the group its own file gives it a seam to grow into
//! without pushing the top-level match over the limit again.

use anyhow::Result;

use super::dispatch::{print_minted_proof, resolve_completion_proof};
use super::types::{OutputCommands, StageCommands};
use crate::commands::stage;

/// `loom stage {block,reset,waiting,resume,hold,release,skip}` dispatch.
///
/// Split out of `dispatch_stage`, which — like `dispatch_knowledge` before
/// it — sits at its own line ceiling: this state-transition group is a
/// coherent seam a new stage subcommand can grow into without pushing the
/// parent match over the limit again.
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
///
/// Split out of `dispatch_stage` for the same reason as
/// `dispatch_stage_transitions`: these three commands are the
/// criteria-adjudication lifecycle — dispute a criterion, adjudicate the
/// dispute, then amend the stage with the verdict — and grouping them gives
/// that lifecycle a seam to grow into without pushing the parent match over
/// its line ceiling again.
fn dispatch_stage_criteria(command: StageCommands) -> Result<()> {
    match command {
        StageCommands::DisputeCriteria {
            stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        } => stage::dispute_criteria(
            stage_id,
            criterion_index,
            reason,
            evidence_commit,
            failure_output,
        ),
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

/// `loom stage <subcommand>` dispatch.
///
/// Extracted for the same reason as `dispatch_knowledge`: the stage group is
/// the largest arm in the top-level match, which sits at its line ceiling.
pub(super) fn dispatch_stage(command: StageCommands) -> Result<()> {
    match command {
        StageCommands::Complete {
            stage_id,
            session,
            no_verify,
            force_unsafe,
            assume_merged,
        } => {
            let admin_proof =
                resolve_completion_proof(&stage_id, no_verify, force_unsafe, assume_merged)?;
            stage::complete(
                stage_id,
                session,
                no_verify,
                force_unsafe,
                assume_merged,
                admin_proof,
            )
        }
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
        StageCommands::HumanReview {
            stage_id,
            approve,
            force_complete,
            reject,
        } => stage::human_review(stage_id, approve, force_complete, reject),
        cmd @ (StageCommands::DisputeCriteria { .. }
        | StageCommands::Adjudicate { .. }
        | StageCommands::Amend { .. }) => dispatch_stage_criteria(cmd),
        StageCommands::Output { command } => match command {
            OutputCommands::Set {
                stage_id,
                key,
                value,
                description,
            } => stage::output_set(stage_id, key, value, description),
            OutputCommands::Get { stage_id, key } => stage::output_get(stage_id, key),
            OutputCommands::List { stage_id } => stage::output_list(stage_id),
            OutputCommands::Remove { stage_id, key } => stage::output_remove(stage_id, key),
        },
    }
}
