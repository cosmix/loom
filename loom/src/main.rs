mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{dispatch, Cli};
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> Result<()> {
    // Recover terminal state if a previous TUI was killed without cleanup
    loom::utils::recover_terminal_if_needed();

    // Initialize tracing subscriber
    // Default level: warn, loom modules at info
    // Configurable via RUST_LOG env var
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,loom=info"));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();

    let cli = Cli::parse();
    dispatch(cli.command)
}
