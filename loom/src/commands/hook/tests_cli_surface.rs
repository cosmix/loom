//! Parse-level checks that removed `loom hook` surface stays removed.

use crate::cli::Cli;
use clap::Parser;

#[test]
fn removed_hook_flags_and_subcommands_do_not_parse() {
    assert!(Cli::try_parse_from(["loom", "hook", "read-receipt", "--check"]).is_err());
    assert!(
        Cli::try_parse_from(["loom", "hook", "worker-brief", "--bind-agent", "agent-1"]).is_err()
    );
    assert!(Cli::try_parse_from(["loom", "hook", "worker-brief"]).is_ok());
}
