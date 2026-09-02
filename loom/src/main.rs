use anyhow::Result;
use clap::Parser;
use loom::cli::{dispatch, Cli};
use tracing_subscriber::{fmt, EnvFilter};

/// Subcommands whose stdout is a protocol rather than a display.
///
/// These are machine-to-machine entry points driven from Claude Code hooks:
/// `loom hook user-prompt` prints a JSON object that the harness injects into an
/// agent's context, and `loom context ...` is invoked the same way. `loom
/// config -k <key>` prints a bare value meant for a script to read. Terminal
/// recovery writes ANSI escape sequences to stdout, which on these commands
/// would be bytes inside that payload rather than a restored display.
const MACHINE_PROTOCOL_COMMANDS: [&str; 3] = ["hook", "context", "config"];

/// Subcommands that must never check for updates or print a notice.
///
/// `hook` and `context` are machine protocol entry points (see
/// [`MACHINE_PROTOCOL_COMMANDS`]) invoked from every Claude Code hook — an
/// update notice on stderr is tolerable there in principle, but there is no
/// value in checking on every single hook call, so they are excluded too.
/// `complete` runs at the tail of a stage and should stay quiet. `run` is the
/// daemon's own parent process — the daemon daemonizes in-process via
/// `fork()`/`setsid()` (`daemon/server/lifecycle.rs`) rather than re-exec'ing
/// `loom`, so there is no second entry point to gate here.
const UPDATE_SILENT_COMMANDS: [&str; 4] = ["hook", "context", "complete", "run"];

/// True when `first_arg` names an [`UPDATE_SILENT_COMMANDS`] subcommand. Read
/// from argv rather than a parsed `Cli`, same rationale as
/// [`writes_a_machine_protocol`]: the decision is made before parsing.
fn suppresses_update_check(first_arg: Option<&str>) -> bool {
    first_arg.is_some_and(|arg| UPDATE_SILENT_COMMANDS.contains(&arg))
}

fn main() -> Result<()> {
    let first_arg = std::env::args().nth(1);

    // The detached refresh child re-enters as `loom __update-refresh`
    // (`loom::update_check::REFRESH_ARG`), not a clap subcommand, so it must
    // be intercepted before anything else — including terminal recovery and
    // tracing init, neither of which this silent, `/dev/null`-piped child
    // needs.
    if first_arg.as_deref() == Some(loom::update_check::REFRESH_ARG) {
        loom::update_check::run_refresh();
        return Ok(());
    }

    // Recover terminal state if a previous TUI was killed without cleanup —
    // before anything else, so a corrupted terminal is fixed before a command
    // renders into it. Exempt: the entry points whose stdout is a protocol.
    if !writes_a_machine_protocol(first_arg.as_deref()) {
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

    // Best-effort, silent update notice — see `loom::update_check`'s module
    // docs for why this takes no network call on the hot path.
    if !suppresses_update_check(first_arg.as_deref()) {
        loom::update_check::notify_and_maybe_refresh();
    }

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

    #[test]
    fn machine_and_orchestration_commands_never_notify() {
        for silent in UPDATE_SILENT_COMMANDS {
            assert!(suppresses_update_check(Some(silent)), "{silent}");
        }
    }

    #[test]
    fn interactive_commands_still_notice_a_release() {
        for interactive in ["status", "plan", "--help", "--version"] {
            assert!(
                !suppresses_update_check(Some(interactive)),
                "{interactive} should still get an update notice"
            );
        }
        assert!(!suppresses_update_check(None), "a bare `loom` invocation");
    }
}
