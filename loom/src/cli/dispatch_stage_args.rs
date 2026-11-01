//! Private stage command families routed from the public Clap enum.

use std::path::PathBuf;

use super::types_stage::{AmendField, AmendOp, DisputeCriteriaArgs, StageCommands};

pub(super) enum TransitionCommand {
    Block(String, String),
    Reset(String, bool, bool),
    Waiting(String),
    Resume(String),
    Hold(String),
    Release(String),
    Skip(String, Option<String>),
}

impl TryFrom<StageCommands> for TransitionCommand {
    type Error = ();

    fn try_from(command: StageCommands) -> Result<Self, Self::Error> {
        match command {
            StageCommands::Block { stage_id, reason } => Ok(Self::Block(stage_id, reason)),
            StageCommands::Reset {
                stage_id,
                hard,
                kill_session,
            } => Ok(Self::Reset(stage_id, hard, kill_session)),
            StageCommands::Waiting { stage_id } => Ok(Self::Waiting(stage_id)),
            StageCommands::Resume { stage_id } => Ok(Self::Resume(stage_id)),
            StageCommands::Hold { stage_id } => Ok(Self::Hold(stage_id)),
            StageCommands::Release { stage_id } => Ok(Self::Release(stage_id)),
            StageCommands::Skip { stage_id, reason } => Ok(Self::Skip(stage_id, reason)),
            _ => Err(()),
        }
    }
}

pub(super) enum CriteriaCommand {
    Dispute(DisputeCriteriaArgs),
    Adjudicate(String, u32, PathBuf),
    Amend(
        String,
        AmendField,
        AmendOp,
        usize,
        Option<String>,
        Option<String>,
    ),
}

impl TryFrom<StageCommands> for CriteriaCommand {
    type Error = ();

    fn try_from(command: StageCommands) -> Result<Self, Self::Error> {
        match command {
            StageCommands::DisputeCriteria(args) => Ok(Self::Dispute(args)),
            StageCommands::Adjudicate {
                stage,
                dispute,
                verdict_file,
            } => Ok(Self::Adjudicate(stage, dispute, verdict_file)),
            StageCommands::Amend {
                stage_id,
                field,
                op,
                index,
                value,
                reason,
            } => Ok(Self::Amend(stage_id, field, op, index, value, reason)),
            _ => Err(()),
        }
    }
}

pub(super) struct CompleteArgs {
    pub stage_id: String,
    pub session: Option<String>,
    pub no_verify: bool,
    pub force_unsafe: bool,
    pub assume_merged: bool,
    pub no_cache: bool,
}

impl TryFrom<StageCommands> for CompleteArgs {
    type Error = ();

    fn try_from(command: StageCommands) -> Result<Self, Self::Error> {
        match command {
            StageCommands::Complete {
                stage_id,
                session,
                no_verify,
                force_unsafe,
                assume_merged,
                no_cache,
            } => Ok(Self {
                stage_id,
                session,
                no_verify,
                force_unsafe,
                assume_merged,
                no_cache,
            }),
            _ => Err(()),
        }
    }
}

pub(super) struct AdminProofArgs {
    pub stage_id: Option<String>,
    pub daemon_stop: bool,
    pub no_verify: bool,
    pub force_unsafe: bool,
    pub assume_merged: bool,
}

impl TryFrom<StageCommands> for AdminProofArgs {
    type Error = ();

    fn try_from(command: StageCommands) -> Result<Self, Self::Error> {
        match command {
            StageCommands::AdminProof {
                stage_id,
                daemon_stop,
                no_verify,
                force_unsafe,
                assume_merged,
            } => Ok(Self {
                stage_id,
                daemon_stop,
                no_verify,
                force_unsafe,
                assume_merged,
            }),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parsed_skip_keeps_optional_reason() {
        let parsed = super::super::types::Cli::try_parse_from([
            "loom",
            "stage",
            "skip",
            "stage-one",
            "--reason",
            "operator decision",
        ])
        .unwrap();
        let super::super::types::Commands::Stage { command } = parsed.command else {
            panic!("expected stage command");
        };
        let TransitionCommand::Skip(stage_id, reason) =
            super::super::dispatch_args::narrow_family(command).unwrap()
        else {
            panic!("skip routed to another transition");
        };
        assert_eq!(stage_id, "stage-one");
        assert_eq!(reason.as_deref(), Some("operator decision"));
    }
}
