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
mod wait;

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
        Some(id) => attach_direct(&work_dir, &sessions, &id),
        None => run_overview(&work_dir),
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

/// Wait for the newest live session of `stage_id` to become attachable,
/// re-reading the live set from disk on every poll (see
/// `wait::poll_for_endpoint`) scoped to THIS stage: a session belonging to a
/// different stage appearing or ending is none of this attach's business,
/// but a newer session for THIS stage replacing the one just picked must be
/// picked up, and every match for this stage ending must trigger the early
/// exit rather than the full timeout. Split out of `attach_direct` only to
/// keep that function under the house size limit.
fn wait_for_stage_target(work_dir: &Path, stage_id: &str) -> Result<Session> {
    let stage_id_owned = stage_id.to_string();
    let work_dir_owned = work_dir.to_path_buf();
    let live_matches_for_stage = move || -> Result<Vec<Session>> {
        let all = viewer::live_tmux_sessions(&work_dir_owned)?;
        Ok(matches_for_stage(&all, &stage_id_owned)
            .into_iter()
            .cloned()
            .collect())
    };
    // Same precondition the overview applies, for the same reason and with
    // the same both-directions caveat — see `viewer::endpoint_ready`.
    let probe = |candidates: &[Session]| -> Option<Session> {
        let refs: Vec<&Session> = candidates.iter().collect();
        let target = pick_newest(&refs)?;
        let tmux_session = tmux_session_name(target).unwrap_or_default();
        endpoint_ready(target, &tmux_session).then(|| target.clone())
    };

    let deadline = wait::endpoint_wait_deadline();
    let outcome = wait::poll_for_endpoint(
        live_matches_for_stage,
        probe,
        deadline,
        wait::ENDPOINT_POLL,
        |count| wait::announce_wait(deadline, count),
    )?;

    match outcome {
        wait::WaitOutcome::Ready(session) => Ok(session),
        wait::WaitOutcome::Ended => bail!(
            "The live session for stage '{stage_id}' ended in {} while loom attach was waiting \
             for its tmux server to accept clients.",
            work_dir.display()
        ),
        wait::WaitOutcome::TimedOut(sessions) => {
            bail!("{}", wait::diagnose_sessions(work_dir, &sessions))
        }
    }
}

/// Attach straight into the session hosting `stage_id`.
fn attach_direct(work_dir: &Path, sessions: &[Session], stage_id: &str) -> Result<()> {
    let matches = matches_for_stage(sessions, stage_id);
    let match_count = matches.len();

    if pick_newest(&matches).is_none() {
        bail!(
            "No live tmux session for stage '{stage_id}'. Live stage ids: {}",
            live_stage_ids(sessions)
        );
    }

    // Reported before the TTY check so it reads as a diagnostic rather than
    // as the `exec` failing: without it, `exec` replaces this process and
    // tmux's own "no server running on ..." becomes the only thing the
    // operator sees.
    let target = wait_for_stage_target(work_dir, stage_id)?;

    if match_count > 1 {
        println!(
            "Found {match_count} live sessions for stage '{stage_id}'; attaching to the newest \
             (session {})",
            target.id
        );
    }

    // Discovery already guaranteed `Some` for every session here.
    let tmux_session = tmux_session_name(&target).unwrap_or_default();

    // Not `find_session_for_stage`: it returns the FIRST session file in
    // filesystem order without checking liveness. The live set above is correct.
    require_tty()?;

    exec_tmux(&[
        "-L",
        &socket_name(&target),
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

/// Build the wording for an empty live set, naming the resolved work dir so
/// "wrong repo of two" reads differently from "backend is actually broken" —
/// without it both looked identical (see
/// `doc/loom/knowledge/mistakes/tmux-backend.md`). Split out from
/// `report_no_live_sessions` purely so the text is testable without a
/// `.work/config.toml` on disk.
fn no_live_sessions_message(work_dir: &Path, backend: SessionBackendKind) -> String {
    match backend {
        SessionBackendKind::Native => format!(
            "loom attach requires the tmux backend for {} (set [terminal] backend = \"tmux\" \
             in .work/config.toml or run loom run --backend tmux)",
            work_dir.display()
        ),
        SessionBackendKind::Tmux => {
            format!(
                "No live tmux sessions in {} (backend: tmux)",
                work_dir.display()
            )
        }
    }
}

/// Explain an empty live set, choosing the message from the CONFIGURED
/// backend. Consulted ONLY here, to pick the wording — gating the whole
/// command on it would be wrong, since live tmux-hosted sessions from before
/// a config flip to native must stay attachable.
fn report_no_live_sessions(work_dir: &Path) -> Result<()> {
    let config = crate::fs::work_dir::read_terminal_config(work_dir)?;
    let message = no_live_sessions_message(work_dir, config.backend);
    match config.backend {
        SessionBackendKind::Native => bail!("{message}"),
        SessionBackendKind::Tmux => {
            println!("{message}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
