//! Reading the text of a session's tmux pane.
//!
//! This is a read of text for a report (what a parked stage's pane showed) or
//! for the history file kept past the session (`super::history`), never a
//! liveness signal: the monitor derives life and death from verified process
//! identity only (see `TmuxBackend::is_session_alive`) and must never consult
//! this module for it.

use std::process::Output;
use std::time::Duration;

use anyhow::Result;

use crate::models::session::Session;
use crate::process::run_bounded_tail_output;

use super::viewer::tmux_session_name;
use super::{
    run_tmux_control, socket_name, socket_path_for, tmux_control_command, TMUX_PROBE_TIMEOUT,
};

/// Pure builder for the `tmux capture-pane` argv (excluding the `tmux`
/// binary): joined wrapped lines (`-J`), the last `lines` rows of scrollback
/// plus the visible screen (`-S -<lines>`). `target` is passed to `-t` as it
/// is (see [`exact_pane_target`]). Arguments are passed as argv elements, so
/// nothing is shell-quoted.
pub(super) fn capture_pane_argv(socket: &str, target: &str, lines: usize) -> Vec<String> {
    ["-L", socket, "capture-pane", "-p", "-J", "-t", target, "-S"]
        .iter()
        .map(|part| (*part).to_string())
        .chain(std::iter::once(format!("-{lines}")))
        .collect()
}

/// Pure builder for the argv that reads a pane's whole scrollback with its
/// colors: `-e` keeps the SGR sequences, `-J` joins wrapped lines, and
/// `-S -` / `-E -` run from the start of the history to the end of the screen.
pub(super) fn capture_history_argv(socket: &str, target: &str) -> Vec<String> {
    [
        "-L",
        socket,
        "capture-pane",
        "-p",
        "-e",
        "-J",
        "-t",
        target,
        "-S",
        "-",
        "-E",
        "-",
    ]
    .iter()
    .map(|part| (*part).to_string())
    .collect()
}

/// The `capture-pane` target for the active pane of the session named exactly
/// `session_name`. tmux resolves a bare name by exact match, then by prefix, then
/// by glob, so a session that is gone could be answered by another whose name
/// starts the same way. A leading `=` allows an exact match only, and the
/// trailing `:` makes it a window target (the session's current window, so its
/// active pane) instead of a pane or window name.
fn exact_pane_target(session_name: &str) -> String {
    format!("={session_name}:")
}

/// What `argv` (built from the socket and pane target) prints for `session`'s
/// own tmux pane, run through `run`, or `None` when the session has no tmux name,
/// its server socket does not exist (a dead or never-created server costs no
/// subprocess), or the capture fails.
///
/// Bounded by `TMUX_PROBE_TIMEOUT` because the orchestrator's single poll
/// thread calls it.
fn capture_stdout(
    session: &Session,
    argv: impl FnOnce(&str, &str) -> Vec<String>,
    run: impl FnOnce(&[&str], Duration, String) -> Result<Output>,
) -> Option<Vec<u8>> {
    let session_name = tmux_session_name(session)?;
    let socket = socket_name(session);
    if !socket_path_for(&socket).exists() {
        return None;
    }
    let args = argv(&socket, &exact_pane_target(&session_name));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(
        &args,
        TMUX_PROBE_TIMEOUT,
        format!("tmux capture-pane ({socket})"),
    )
    .ok()?;
    output.status.success().then_some(output.stdout)
}

/// The raw text of the last `lines` rows of `session`'s own tmux pane, or
/// `None` when it cannot be read. The text is returned as captured; callers
/// normalise it.
pub(crate) fn capture_pane_tail(session: &Session, lines: usize) -> Option<String> {
    capture_stdout(
        session,
        |socket, target| capture_pane_argv(socket, target, lines),
        run_tmux_control,
    )
    .map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
}

fn run_tail(args: &[&str], timeout: Duration, operation: String) -> Result<Output> {
    let mut command = tmux_control_command(args);
    run_bounded_tail_output(&mut command, timeout, operation)
}

/// `session`'s whole scrollback and screen, colors included, or `None` when
/// the pane cannot be read. A scrollback past the capture limit keeps its end:
/// `history::bound_history` keeps the end anyway, so its start cannot cost the
/// session its whole history.
pub(super) fn capture_history(session: &Session) -> Option<Vec<u8>> {
    capture_stdout(session, capture_history_argv, run_tail)
}

/// What a pane shows, read in one tmux server operation.
pub(super) enum PaneScreen {
    /// The pane is on its alternate screen (a fullscreen program); the bytes
    /// are the visible screen with colors.
    Alternate(Vec<u8>),
    /// The pane shows its main screen; no visible bytes are kept.
    Main,
}

/// The tmux reads the history writers need, injectable so their decisions can
/// be exercised without a tmux server.
pub(super) trait PaneReads {
    /// In one tmux server operation on the session's exact pane target:
    /// `Some(Alternate(visible bytes))` while the pane is still on its
    /// alternate screen, `Some(Main)` once it shows its main screen, `None`
    /// when the pane cannot be read.
    fn alternate_screen(&self, session: &Session) -> Option<PaneScreen>;
    /// The whole scrollback with colors, `None` when it cannot be read.
    fn scrollback(&self, session: &Session) -> Option<Vec<u8>>;
}

/// The real tmux reads.
pub(super) struct TmuxPane;

impl PaneReads for TmuxPane {
    fn alternate_screen(&self, session: &Session) -> Option<PaneScreen> {
        // The target is embedded in a tmux command string below; a name that
        // could end its quoting is refused rather than escaped.
        let name = tmux_session_name(session)?;
        if name.contains(['\'', '\\', '\n']) {
            return None;
        }
        capture_stdout(session, alternate_screen_argv, run_tail)
            .and_then(|stdout| parse_alternate_screen(&stdout))
    }

    fn scrollback(&self, session: &Session) -> Option<Vec<u8>> {
        capture_history(session)
    }
}

/// Pure builder for the argv that answers, in one tmux operation, whether the
/// pane is on its alternate screen and, only then, what it shows. `if-shell -F`
/// tests `#{alternate_on}` and runs the matching branch inside the same server
/// command queue, so a pane that restores its main screen between a separate
/// check and a separate capture cannot slip through. The branches print a
/// leading `1` (alternate, then the visible screen with colors, wrapped lines
/// joined) or `0` (main, nothing more). `target` must hold no `'`.
pub(super) fn alternate_screen_argv(socket: &str, target: &str) -> Vec<String> {
    let alternate = format!("display-message -p 1 ; capture-pane -p -e -J -t '{target}'");
    [
        "-L",
        socket,
        "if-shell",
        "-F",
        "-t",
        target,
        "#{alternate_on}",
        alternate.as_str(),
        "display-message -p 0",
    ]
    .iter()
    .map(|part| (*part).to_string())
    .collect()
}

/// Read the output of [`alternate_screen_argv`]: anything that does not lead
/// with its `1` or `0` line is no answer.
fn parse_alternate_screen(stdout: &[u8]) -> Option<PaneScreen> {
    if let Some(visible) = stdout.strip_prefix(b"1\n") {
        Some(PaneScreen::Alternate(visible.to_vec()))
    } else if stdout.starts_with(b"0\n") {
        Some(PaneScreen::Main)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pane_target_names_its_session_exactly() {
        assert_eq!(exact_pane_target("loom-stage"), "=loom-stage:");
    }

    #[test]
    fn the_history_argv_reads_the_whole_scrollback_with_colors() {
        let argv = capture_history_argv("loom-socket", &exact_pane_target("loom-stage"));
        assert_eq!(
            argv,
            [
                "-L",
                "loom-socket",
                "capture-pane",
                "-p",
                "-e",
                "-J",
                "-t",
                "=loom-stage:",
                "-S",
                "-",
                "-E",
                "-"
            ]
        );
    }

    #[test]
    fn the_alternate_screen_read_is_one_conditional_operation_on_the_exact_target() {
        let argv = alternate_screen_argv("loom-socket", &exact_pane_target("loom-stage"));
        assert_eq!(
            argv,
            [
                "-L",
                "loom-socket",
                "if-shell",
                "-F",
                "-t",
                "=loom-stage:",
                "#{alternate_on}",
                "display-message -p 1 ; capture-pane -p -e -J -t '=loom-stage:'",
                "display-message -p 0",
            ]
        );
    }

    #[test]
    fn the_alternate_screen_answer_is_its_leading_line() {
        assert!(matches!(
            parse_alternate_screen(b"1\nvisible\n"),
            Some(PaneScreen::Alternate(visible)) if visible == b"visible\n"
        ));
        assert!(matches!(
            parse_alternate_screen(b"0\n"),
            Some(PaneScreen::Main)
        ));
        assert!(parse_alternate_screen(b"").is_none());
        assert!(parse_alternate_screen(b"no server running\n").is_none());
    }

    #[test]
    fn the_capture_argv_carries_the_exact_target() {
        let argv = capture_pane_argv("loom-socket", &exact_pane_target("loom-stage"), 40);
        let target = argv.iter().position(|arg| arg == "-t").unwrap() + 1;
        assert_eq!(argv[target], "=loom-stage:");
    }
}
