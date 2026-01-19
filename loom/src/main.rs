mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{dispatch, Cli};

fn main() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli.command)
}
