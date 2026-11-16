use std::{io::Write, path::PathBuf};

use crate::native_worker::{ids::validate_unit_id, model::Delivery};

#[derive(Debug, clap::Args)]
pub struct StartArgs {
    #[arg(long, value_parser = ["codex"])]
    pub provider: String,
    #[arg(long = "unit-id")]
    pub unit: String,
    #[arg(long)]
    pub brief: PathBuf,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub effort: String,
}

#[derive(Debug, clap::Args)]
pub struct MessageArgs {
    #[arg(long)]
    pub worker: String,
    #[arg(long)]
    pub invocation: String,
    #[arg(long, value_enum)]
    pub delivery: DeliveryArg,
    #[arg(long)]
    pub message_id: String,
    #[arg(long, conflicts_with = "file", required_unless_present = "file")]
    pub text: Option<String>,
    #[arg(long, conflicts_with = "text")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct CancelArgs {
    #[arg(long)]
    pub worker: String,
    #[arg(long, conflicts_with = "stage", required_unless_present = "stage")]
    pub invocation: Option<String>,
    #[arg(
        long,
        conflicts_with = "invocation",
        required_unless_present = "invocation"
    )]
    pub stage: Option<String>,
    #[arg(long)]
    pub reason: String,
}

#[derive(Debug, clap::Args)]
pub struct ReleaseArgs {
    #[arg(long)]
    pub stage: String,
    #[arg(long)]
    pub invocation: String,
}

#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub worker: String,
    #[arg(long)]
    pub stage: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct ResultArgs {
    #[arg(long)]
    pub worker: String,
    #[arg(long)]
    pub stage: Option<String>,
    #[arg(long)]
    pub full: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DeliveryArg {
    Steer,
    NextTurn,
}

impl From<DeliveryArg> for Delivery {
    fn from(value: DeliveryArg) -> Self {
        match value {
            DeliveryArg::Steer => Self::Steer,
            DeliveryArg::NextTurn => Self::NextTurn,
        }
    }
}

pub(super) const EXIT_REFUSED: i32 = 3;

pub(super) fn parse_worker(worker: &str) -> Result<String, String> {
    worker
        .strip_prefix("codex:")
        .filter(|unit| validate_unit_id(unit).is_ok())
        .map(str::to_owned)
        .ok_or_else(|| format!("--worker must be codex:<unit-id>, got '{worker}'"))
}

pub(super) fn refuse(err: &mut dyn Write, command: &str, reason: &str) -> i32 {
    let _ = writeln!(err, "loom subagents {command}: {reason}");
    EXIT_REFUSED
}
