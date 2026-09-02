use anyhow::Result;
use clap::Parser;
use loom::cli::{dispatch, Cli};
use tracing_subscriber::{fmt, EnvFilter};

/// Subcommands whose stdout is a protocol rather than a display.
///
/// These are machine-to-machine entry points driven from Claude Code hooks:
/// `loom hook user-prompt` prints a JSON object that the harness injects into an
/// agent's context, and `loom context ...` is invoked the same way. Terminal
/// recovery writes ANSI escape sequences to stdout, which on these commands
/// would be bytes inside that payload rather than a restored display.
const MACHINE_PROTOCOL_COMMANDS: [&str; 2] = ["hook", "context"];

fn main() -> Result<()> {
    // Recover terminal state if a previous TUI was killed without cleanup —
    // before anything else, so a corrupted terminal is fixed before a command
    // renders into it. Exempt: the entry points whose stdout is a protocol.
    if !writes_a_machine_protocol(std::env::args().nth(1).as_deref()) {
        loom::utils::recover_terminal_if_needed();
    }

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

/// True when `first_arg` names a [`MACHINE_PROTOCOL_COMMANDS`] subcommand.
///
/// Read from argv rather than from a parsed `Cli` because the decision is made
/// before parsing. That is exact here: the first argument is always the
/// subcommand, or a flag starting with `-` (`--help`, `-v`, `--version`), and
/// anything starting with `-` matches nothing in the list.
fn writes_a_machine_protocol(first_arg: Option<&str>) -> bool {
    first_arg.is_some_and(|arg| MACHINE_PROTOCOL_COMMANDS.contains(&arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_entry_points_are_exempt_from_terminal_recovery() {
        assert!(writes_a_machine_protocol(Some("hook")));
        assert!(writes_a_machine_protocol(Some("context")));
    }

    #[test]
    fn interactive_commands_still_recover_the_terminal() {
        for interactive in ["status", "run", "init", "--help", "--version"] {
            assert!(
                !writes_a_machine_protocol(Some(interactive)),
                "{interactive} renders for a human and must still recover"
            );
        }
        assert!(!writes_a_machine_protocol(None), "a bare `loom` invocation");
    }
}
