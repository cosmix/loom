//! Tmux session-reaping helpers for `loom clean`.
//!
//! Split out of `commands::clean` to keep that module's file under the
//! 400-line cap: this owns the tmux socket sweep, while `worktrees.rs` owns
//! git worktree/branch cleanup.

use anyhow::Result;
use colored::Colorize;
use std::{fs, path::Path};

use crate::models::stage::StageStatus;
use crate::orchestrator::terminal::native::{cleanup_stage_files, NativeBackend};
use crate::orchestrator::terminal::tmux::{
    kill_socket_server, list_loom_sockets, socket_session_is_alive, LoomSocket,
};

/// Controls whether [`clean_sessions`] reaps only dead sessions or also live
/// ones. Mirrors `commands::init::cleanup::SessionReapMode`, which runs the
/// identical two-mode sweep at `loom init` — kept as a separate type here
/// because that module is private to `commands::init`, so it cannot be
/// imported directly without widening that module's visibility (out of
/// scope for this fix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionReapMode {
    /// Reap only attributed sockets whose session is no longer alive.
    OrphansOnly,
    /// Reap every attributed socket, alive or not. Selected by `execute`
    /// whenever this invocation is about to delete the state directory, which is the
    /// only thing that makes a socket attributable back to this repo in the
    /// first place — reaping must happen before that record is destroyed, or
    /// a live tmux-hosted session leaks forever with no way to find it again.
    IncludeLiveBeforeClean,
}

/// Outcome of [`handle_socket`] deciding what to do with a single socket.
enum SocketOutcome {
    /// Not attributed to this work dir — never touched.
    Unattributed,
    /// Attributed but left alone (an `OrphansOnly` sweep skipping a live one).
    Kept,
    /// Attributed and reaped; `was_alive` distinguishes a genuinely dead
    /// (orphaned) session from a live one reaped early by
    /// `IncludeLiveBeforeClean`.
    Reaped { was_alive: bool },
}

/// Decide what to do with one socket found by `list_loom_sockets`, and act on
/// that decision. Split out of [`clean_sessions`] so the sweep/report loop
/// there stays small; this owns the per-socket policy. Mirrors
/// `commands::init::cleanup::cleanup_orphaned_sessions`'s per-socket handling
/// (kept separate rather than shared — see [`SessionReapMode`]'s docs).
///
/// If `kill_socket_server` fails while the session still appeared alive, the
/// socket is still removed (a failed kill against an already-dead socket is
/// the COMMON case for a genuinely orphaned session, so unconditional removal
/// is what actually reaps stale sockets) but the operator is warned, since
/// discarding the file in that specific case may be the only handle to a
/// server that is genuinely still running.
fn handle_socket(work_dir: &Path, socket: &LoomSocket, mode: SessionReapMode) -> SocketOutcome {
    if !socket.attributed {
        return SocketOutcome::Unattributed;
    }

    let alive = socket_session_is_alive(work_dir, &socket.session_id);
    if alive && mode == SessionReapMode::OrphansOnly {
        return SocketOutcome::Kept;
    }

    let killed = kill_socket_server(&socket.path);
    if alive && !killed {
        println!(
            "  {} kill-server failed for session {} while its process still appears alive; \
             removing the socket anyway — it may leak. Try `tmux -S {} kill-server` manually.",
            "⚠".yellow().bold(),
            socket.session_id,
            socket.path.display()
        );
    }
    let _ = fs::remove_file(&socket.path);
    SocketOutcome::Reaped { was_alive: alive }
}

/// Reaps loom tmux sessions attributed to this work dir, per `mode`.
///
/// LIVE session termination normally stays the exclusive domain of `loom
/// sessions kill` — with [`SessionReapMode::OrphansOnly`] (used for a bare
/// `--sessions`), this only reaps sockets whose session is no longer alive.
/// With [`SessionReapMode::IncludeLiveBeforeClean`] (selected by `execute`
/// whenever this invocation is about to delete the state directory), an attributed
/// socket is reaped even if its session is still alive — deleting the state directory
/// destroys the only record that lets that socket ever be attributed again,
/// so it must be reaped NOW or it leaks forever. Unattributable sockets
/// (which may belong to another checkout or user, since the tmux socket
/// directory is per-user, not per-repo) are reported but left untouched in
/// EITHER mode. Mirrors `commands::init::cleanup::cleanup_orphaned_sessions`,
/// which runs the identical two-mode sweep at `loom init`.
pub(super) fn clean_sessions(repo_root: &Path, mode: SessionReapMode) -> Result<usize> {
    print_sessions_header(mode);

    let work_dir = super::resolve_state_dir(repo_root);
    let mut orphaned_reaped = 0;
    let mut live_reaped = 0;
    let mut unattributed = 0;

    for socket in list_loom_sockets(&work_dir) {
        match handle_socket(&work_dir, &socket, mode) {
            SocketOutcome::Unattributed => unattributed += 1,
            SocketOutcome::Kept => {}
            SocketOutcome::Reaped { was_alive: true } => live_reaped += 1,
            SocketOutcome::Reaped { was_alive: false } => orphaned_reaped += 1,
        }
    }

    cleanup_terminal_tombstones(&work_dir, mode)?;

    print_sessions_summary(orphaned_reaped, live_reaped, unattributed);
    Ok(orphaned_reaped + live_reaped)
}

/// Remove PID and wrapper tombstones only when a session record positively
/// attributes them to a finished stage (or when `loom clean` will remove that
/// record together with the state directory). Never scan the PID or wrapper directories:
/// an unparseable or otherwise unattributed entry is evidence we must retain.
fn cleanup_terminal_tombstones(work_dir: &Path, mode: SessionReapMode) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&sessions_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        let Some(session) = read_session_for_tombstone_cleanup(&path) else {
            continue;
        };

        if mode != SessionReapMode::IncludeLiveBeforeClean && !stage_is_terminal(&session, work_dir)
        {
            continue;
        }

        if let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(&session) {
            cleanup_stage_files(work_dir, &pid_key);
        }
    }

    Ok(())
}

fn read_session_for_tombstone_cleanup(path: &Path) -> Option<crate::models::session::Session> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "unable to read session record while reaping terminal tombstones");
            return None;
        }
    };
    match crate::parser::frontmatter::parse_from_markdown(&content, "Session") {
        Ok(session) => Some(session),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "unable to parse session record while reaping terminal tombstones");
            None
        }
    }
}

/// A session's tombstones are eligible for cleanup only after its assigned
/// stage is permanently complete. A missing or unreadable stage is not proof
/// of completion and must leave the attributed files intact.
fn stage_is_terminal(session: &crate::models::session::Session, work_dir: &Path) -> bool {
    let Some(stage_id) = session.stage_id.as_deref() else {
        return false;
    };
    let Ok(stage) = crate::verify::transitions::load_stage(stage_id, work_dir) else {
        return false;
    };

    (stage.status == StageStatus::Completed && stage.merged) || stage.status == StageStatus::Skipped
}

/// Explains, before the sweep runs, why this invocation may reap a LIVE
/// session — split out of [`clean_sessions`] purely to keep that function
/// under the line-count cap.
fn print_sessions_header(mode: SessionReapMode) {
    if mode == SessionReapMode::OrphansOnly {
        println!(
            "  {} --sessions no longer terminates sessions; use 'loom sessions kill'",
            "─".dimmed()
        );
    } else {
        println!(
            "  {} the state directory is about to be removed — reaping attributed sessions \
             (live or not) so none leak unattributed",
            "─".dimmed()
        );
    }
}

/// Reports the outcome of [`clean_sessions`]'s sweep — split out for the same
/// reason as [`print_sessions_header`].
fn print_sessions_summary(orphaned_reaped: usize, live_reaped: usize, unattributed: usize) {
    let total_reaped = orphaned_reaped + live_reaped;
    if total_reaped > 0 && live_reaped == 0 {
        println!(
            "  {} Reaped {} orphaned tmux session{}",
            "✓".green().bold(),
            orphaned_reaped,
            if orphaned_reaped == 1 { "" } else { "s" }
        );
    } else if live_reaped > 0 {
        println!(
            "  {} Reaped {} tmux session{} ({} orphaned, {} still live — about to be \
             unattributable once the state directory is removed)",
            "✓".green().bold(),
            total_reaped,
            if total_reaped == 1 { "" } else { "s" },
            orphaned_reaped,
            live_reaped
        );
    }

    if unattributed > 0 {
        println!(
            "  {} {} unattributable tmux socket{} left untouched",
            "─".dimmed(),
            unattributed,
            if unattributed == 1 { "" } else { "s" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::session_to_markdown;
    use crate::models::session::Session;
    use crate::models::stage::{Stage, StageStatus};
    use serial_test::serial;
    use tempfile::TempDir;

    /// Points `TMUX_TMPDIR` at an isolated directory for the duration of the
    /// test and restores it on drop, mirroring
    /// `orchestrator::terminal::tmux::socket`'s own test guard and
    /// `commands::init::tests`'s copy of it. That guard is `pub(crate)`, but
    /// its parent module (`tmux::socket`) is private and re-exports only the
    /// socket API, so it is unreachable from here — hence a copy rather than
    /// an import.
    struct TmuxTmpDirGuard {
        original: Option<std::ffi::OsString>,
    }
    impl TmuxTmpDirGuard {
        fn set(dir: &Path) -> Self {
            let original = std::env::var_os("TMUX_TMPDIR");
            std::env::set_var("TMUX_TMPDIR", dir);
            Self { original }
        }
    }
    impl Drop for TmuxTmpDirGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var("TMUX_TMPDIR", value),
                None => std::env::remove_var("TMUX_TMPDIR"),
            }
        }
    }

    /// Mirrors `commands::init::tests::test_cleanup_orphaned_sessions_reaps_live_only_in_clean_mode`:
    /// a live ATTRIBUTED socket must be reaped once `loom clean` is about to
    /// destroy the state directory (FIX 1's `IncludeLiveBeforeClean` mode), but
    /// preserved when the state directory is not being destroyed; an UNATTRIBUTED
    /// socket must be preserved in BOTH modes since the tmux socket
    /// directory is per-user and it may belong to another checkout entirely.
    #[test]
    #[serial]
    fn test_clean_sessions_reaps_live_only_when_destroying_state() {
        let tmux_tmpdir = TempDir::new().unwrap();
        let _guard = TmuxTmpDirGuard::set(tmux_tmpdir.path());
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let socket_dir = tmux_tmpdir.path().join(format!("tmux-{uid}"));
        fs::create_dir_all(&socket_dir).unwrap();

        let repo_root = TempDir::new().unwrap();
        // No config.toml anywhere under `repo_root`, so `clean_sessions`'s internal
        // `resolve_state_dir` (via `WorkDir::new`) resolves to the nested fallback root.
        let sessions_dir = repo_root.path().join(".loom").join("work").join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        let mut live_session = Session::new();
        live_session.id = "session-cleanlive000".to_string();
        live_session.assign_to_stage("stage-cleanlive000".to_string());
        live_session.pid = Some(std::process::id());
        fs::write(
            sessions_dir.join(format!("{}.md", live_session.id)),
            session_to_markdown(&live_session),
        )
        .unwrap();
        crate::orchestrator::terminal::native::write_test_pid_identity(
            &repo_root.path().join(".loom").join("work"),
            &live_session,
            std::process::id(),
        )
        .unwrap();
        let live_socket = socket_dir.join(format!("loom-{}", live_session.id));
        fs::write(&live_socket, "").unwrap();

        let unattributed_socket = socket_dir.join("loom-session-strangercase1");
        fs::write(&unattributed_socket, "").unwrap();

        clean_sessions(repo_root.path(), SessionReapMode::OrphansOnly).unwrap();
        assert!(
            live_socket.exists(),
            "a live attributed session must never be reaped when the state directory is not \
             being destroyed"
        );
        assert!(
            unattributed_socket.exists(),
            "an unattributed socket must never be touched"
        );

        clean_sessions(repo_root.path(), SessionReapMode::IncludeLiveBeforeClean).unwrap();
        assert!(
            !live_socket.exists(),
            "a live session must be reaped before the state directory destroys its attribution"
        );
        assert!(
            unattributed_socket.exists(),
            "an unattributed socket must never be touched, even when the state directory is \
             about to be destroyed"
        );
    }

    fn write_session_tombstones(work_dir: &Path, sessions_dir: &Path, session: &Session) {
        fs::write(
            sessions_dir.join(format!("{}.md", session.id)),
            session_to_markdown(session),
        )
        .unwrap();
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(session).unwrap();
        crate::orchestrator::terminal::native::create_wrapper_script(
            work_dir,
            &pid_key,
            session.stage_id.as_deref().unwrap(),
            &session.id,
            "claude 'prompt'",
            None,
            session.session_type,
            150_000,
        )
        .unwrap(); // also creates `pids/`
        fs::write(
            work_dir.join("pids").join(format!("{pid_key}.pid")),
            "999999999\n1\n",
        )
        .unwrap();
    }

    fn tombstones_exist(work_dir: &Path, session: &Session) -> (bool, bool) {
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(session).unwrap();
        (
            work_dir
                .join("pids")
                .join(format!("{pid_key}.pid"))
                .exists(),
            work_dir
                .join("wrappers")
                .join(format!("{pid_key}-wrapper.sh"))
                .exists(),
        )
    }

    #[test]
    #[serial]
    fn clean_sessions_reaps_only_tombstones_attributed_to_finished_sessions() {
        let tmux_tmpdir = TempDir::new().unwrap();
        let _guard = TmuxTmpDirGuard::set(tmux_tmpdir.path());
        let repo_root = TempDir::new().unwrap();
        let work_dir = repo_root.path().join(".loom").join("work");
        let sessions_dir = work_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        let mut finished_stage = Stage::new("finished-stage".to_string(), None);
        finished_stage.id = "finished-stage".to_string();
        finished_stage.status = StageStatus::Skipped;
        crate::verify::transitions::save_stage(&finished_stage, &work_dir).unwrap();

        let mut finished_session = Session::new();
        finished_session.assign_to_stage(finished_stage.id.clone());
        let mut running_session = Session::new();
        running_session.assign_to_stage("running-stage".to_string());
        write_session_tombstones(&work_dir, &sessions_dir, &finished_session);
        write_session_tombstones(&work_dir, &sessions_dir, &running_session);
        clean_sessions(repo_root.path(), SessionReapMode::OrphansOnly).unwrap();

        assert_eq!(
            tombstones_exist(&work_dir, &finished_session),
            (false, false)
        );
        assert_eq!(tombstones_exist(&work_dir, &running_session), (true, true));
    }
}
