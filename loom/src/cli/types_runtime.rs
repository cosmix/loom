//! Arguments of the top-level `run`, `status` and `completions` commands.

use std::path::PathBuf;

use clap::Args;

use super::status_web::StatusWebArgs;
use crate::cli::types_help::positive_usize;

#[derive(Args)]
pub struct RunArgs {
    /// Enable manual approval for each stage
    #[arg(short, long)]
    pub manual: bool,

    /// Maximum number of parallel sessions (default: 4)
    #[arg(short = 'p', long, value_parser = positive_usize)]
    pub max_parallel: Option<usize>,

    /// Run orchestrator in foreground (not recommended)
    #[arg(long)]
    pub foreground: bool,

    /// Watch mode: continuously spawn ready stages until all are terminal
    #[arg(short, long, requires = "foreground")]
    pub watch: bool,

    /// Disable auto-merge of completed stages (merge is enabled by default)
    #[arg(long)]
    pub no_merge: bool,

    /// Terminal backend for sessions (native|tmux); persisted to the `[terminal]` section of the loom config
    #[arg(long, value_parser = ["native", "tmux"])]
    pub backend: Option<String>,

    /// Start although a stage sets `allow_all_unix_sockets`; required on every such run
    #[arg(long)]
    pub accept_unix_socket_risk: bool,

    /// Internal: serve as the daemon for this absolute state directory
    #[arg(long, hide = true, conflicts_with = "foreground")]
    pub daemon_child: Option<PathBuf>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Live mode: subscribe to daemon for real-time updates
    #[arg(short, long)]
    pub live: bool,

    /// Compact mode: single-line output for scripting
    #[arg(short, long)]
    pub compact: bool,

    /// Verbose mode: show detailed failure information
    #[arg(short, long)]
    pub verbose: bool,

    #[command(flatten)]
    pub web_args: StatusWebArgs,
}

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for (bash, zsh, fish).
    /// Auto-detected from $SHELL when using --install without specifying a shell.
    pub shell: Option<String>,

    /// Install completions to the appropriate system location
    #[arg(long)]
    pub install: bool,

    /// Check for outdated completions and show migration instructions
    #[arg(long)]
    pub migrate: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::super::{Cli, Commands};

    #[test]
    fn parsed_run_and_status_keep_their_runtime_fields() {
        let run = Cli::try_parse_from(["loom", "run", "--foreground", "--watch"])
            .unwrap()
            .command;
        let Commands::Run(args) = run else {
            panic!("run routed to another command");
        };
        assert!(args.foreground && args.watch);

        let status = Cli::try_parse_from(["loom", "status", "--compact"])
            .unwrap()
            .command;
        let Commands::Status(args) = status else {
            panic!("status routed to another command");
        };
        assert!(args.compact);
    }
}
