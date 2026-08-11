//! `loom attach [stage-id]` — attach to tmux-hosted loom sessions.
//!
//! Direct (`stage_id` given): `exec`s straight into one stage's own tmux
//! server, replacing the loom process with the attach client. Overview
//! (`stage_id` omitted): builds a separate per-repo "viewer" tmux server
//! whose panes each host a nested attach client into one live session's own
//! server, tiled side by side, then `exec`s into the viewer.
//!
//! Native-backend sessions are out of scope: they already own a visible OS
//! terminal window, so there is nothing for this command to attach to.
//!
//! Both paths apply one precondition the rest of loom deliberately does not:
//! the target's tmux server must be accepting clients *now*. That check lives
//! in `orchestrator::terminal::tmux::viewer::endpoint_ready`, shared with the
//! daemon-side reconciler that keeps an attached overview in sync, so the two
//! can never disagree about who is attachable.

mod overview;

use anyhow::{bail, Result};
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::commands::common::find_work_dir;
use crate::models::session::{Session, SessionBackendKind};
use crate::orchestrator::terminal::tmux::socket_name;
use crate::orchestrator::terminal::tmux::viewer::{self, endpoint_ready, tmux_session_name};
use overview::run_overview;

/// Entry point. `stage_id == None` => tiled overview of every live tmux
/// session; `Some(id)` => attach straight into that stage's session.
pub fn execute(stage_id: Option<String>) -> Result<()> {
    let work_dir = find_work_dir()?;
    let sessions = viewer::live_tmux_sessions(&work_dir)?;

    if sessions.is_empty() {
        return report_no_live_sessions(&work_dir);
    }

    match stage_id {
        Some(id) => attach_direct(&sessions, &id),
        None => run_overview(&work_dir, &sessions),
    }
}

/// Every live session assigned to `stage_id`. Split out of `attach_direct`
/// purely for testability — together with [`pick_newest`] it IS the selection
/// invariant this command exists to get right, and unlike the rest of
/// `attach_direct` the pair needs neither tmux, a TTY, nor `exec` to exercise.
fn matches_for_stage<'a>(sessions: &'a [Session], stage_id: &str) -> Vec<&'a Session> {
    sessions
        .iter()
        .filter(|s| s.stage_id.as_deref() == Some(stage_id))
        .collect()
}

/// Pick the most-recently-created session from an already-filtered slice
/// (`max_by_key` returns the LAST maximum, the determinism this command
/// relies on). Kept separate from [`matches_for_stage`] so `attach_direct`
/// can report `matches.len()` without filtering the same predicate twice.
fn pick_newest<'a>(candidates: &[&'a Session]) -> Option<&'a Session> {
    candidates.iter().copied().max_by_key(|s| s.created_at)
}

/// Name every stage that DOES have a live session, so an unknown or misspelled
/// stage id is answered with the choices rather than a bare refusal.
fn live_stage_ids(sessions: &[Session]) -> String {
    let mut live_ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s.stage_id.as_deref())
        .collect();
    live_ids.sort_unstable();
    live_ids.dedup();
    if live_ids.is_empty() {
        "(none)".to_string()
    } else {
        live_ids.join(", ")
    }
}

/// Attach straight into the session hosting `stage_id`.
fn attach_direct(sessions: &[Session], stage_id: &str) -> Result<()> {
    let matches = matches_for_stage(sessions, stage_id);

    let Some(target) = pick_newest(&matches) else {
        bail!(
            "No live tmux session for stage '{stage_id}'. Live stage ids: {}",
            live_stage_ids(sessions)
        );
    };

    if matches.len() > 1 {
        println!(
            "Found {} live sessions for stage '{stage_id}'; attaching to the newest (session {})",
            matches.len(),
            target.id
        );
    }

    // Discovery already guaranteed `Some` for every session here.
    let tmux_session = tmux_session_name(target).unwrap_or_default();

    // Same precondition the overview applies, for the same reason and with
    // the same both-directions caveat — see `viewer::endpoint_ready`. Reported
    // before the TTY check so it reads as a diagnostic rather than as the
    // `exec` failing: without it, `exec` replaces this process and tmux's own
    // "no server running on ..." becomes the only thing the operator sees.
    if !endpoint_ready(target, &tmux_session) {
        bail!(
            "Stage '{stage_id}' has a live session ({}) whose tmux server is not accepting \
             clients — it is still spawning, or has just ended. Re-run in a moment.",
            target.id
        );
    }

    // Not `find_session_for_stage`: it returns the FIRST session file in
    // filesystem order without checking liveness. The live set above is correct.
    require_tty()?;

    exec_tmux(&[
        "-L",
        &socket_name(target),
        "attach-session",
        "-t",
        &tmux_session,
    ])
}

/// `exec` into tmux, replacing the loom process. Only ever returns an error:
/// `CommandExt::exec` returns solely on failure.
fn exec_tmux(argv: &[&str]) -> Result<()> {
    // `.env_remove("TMUX")` so attaching works even when `loom attach` itself
    // is run from inside another tmux session.
    let err = Command::new("tmux").args(argv).env_remove("TMUX").exec();
    Err(anyhow::Error::new(err).context(format!("Failed to exec tmux {}", argv.join(" "))))
}

/// Refuse to exec tmux when stdout is not a terminal — tmux would otherwise
/// fail obscurely ("open terminal failed").
fn require_tty() -> Result<()> {
    if !std::io::stdout().is_terminal() {
        bail!("loom attach must be run from a terminal");
    }
    Ok(())
}

/// Explain an empty live set, choosing the message from the CONFIGURED
/// backend. Consulted ONLY here, to pick the wording — gating the whole
/// command on it would be wrong, since live tmux-hosted sessions from before
/// a config flip to native must stay attachable.
fn report_no_live_sessions(work_dir: &Path) -> Result<()> {
    let config = crate::fs::work_dir::read_terminal_config(work_dir)?;
    match config.backend {
        SessionBackendKind::Native => bail!(
            "loom attach requires the tmux backend (set [terminal] backend = \"tmux\" in .work/config.toml or run loom run --backend tmux)"
        ),
        SessionBackendKind::Tmux => {
            println!("No live tmux sessions");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
