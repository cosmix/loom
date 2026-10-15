//! A tmux session's terminal history, kept on disk after its server is gone.
//!
//! Each session runs on a tmux server of its own, and every teardown ends that
//! server, taking the pane's scrollback with it. [`save_history`] reads the whole
//! scrollback just before [`TmuxBackend::kill_session`] kills the server and
//! writes it to `terminal-history/<session-id>.terminal` under the state root;
//! the status collector and the dashboard read it back through [`has_history`]
//! and [`read_history`].
//!
//! A history holds everything a stage's agent printed, and every session runs as
//! the same user, so two layers keep one stage from reading what another showed.
//! Each session's capsule puts the directory in the OS sandbox's `denyRead`
//! (`session_settings::contents`), which binds Bash. The native Read, Grep and
//! Glob tools run outside that sandbox; `loom-hooks/credential-guard.sh` refuses
//! them any path inside the directory, but only for a session loom launched (its
//! wrapper exports `LOOM_SESSION_ID`, `LOOM_STAGE_ID` and `LOOM_WORK_DIR`), so
//! the operator's own session can still read a transcript. It also refuses a
//! search that would walk into the state root; a session in the main checkout
//! can still reach the directory through a path-less search after a `cd`.
//!
//! Two server options make the capture complete (`SERVER_OPTIONS`). They are
//! applied in the invocation that creates the server, because a pane takes its
//! `history-limit` when it is created: set afterwards, the option would reach
//! only later panes, and the session's one pane would keep tmux's 2000 lines.
//! `remain-on-exit` keeps the pane, and with it the server, after an agent exits
//! on its own, so there is still something to capture. The server then lives
//! until [`TmuxBackend::kill_session`] ends it, which every end path reaches.
//! The dead pane holds only what tmux read before it reaped the agent: tmux
//! closes the pane's terminal then, so output still unread at that moment (a
//! loaded host leaves some) never reaches the history.
//!
//! [`TmuxBackend::kill_session`]: super::TmuxBackend::kill_session

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::safe_fs::{safe_create_dir_all, safe_locked_write, MAX_LOG_BYTES};
use crate::fs::safe_read::{is_not_found, open_regular_no_follow, read_bounded};
use crate::models::session::Session;
use crate::validation::validate_id;

use super::capture::capture_history;

/// Options set on the server in the same invocation that creates it; see the
/// module docs for why that timing matters.
pub(super) const SERVER_OPTIONS: &[(&str, &str)] =
    &[("history-limit", "50000"), ("remain-on-exit", "on")];

/// The most bytes a history file holds, and the most the dashboard serves.
/// It is the limit `safe_fs` puts on one write.
pub const MAX_HISTORY_BYTES: usize = MAX_LOG_BYTES;

/// Resets colors and attributes where a cut history starts mid-stream.
const SGR_RESET: &[u8] = b"\x1b[0m";

/// The directory under the state root that holds every history file.
const HISTORY_DIR: &str = "terminal-history";

/// The history directory of the state root `work_dir`.
pub fn history_dir(work_dir: &Path) -> PathBuf {
    work_dir.join(HISTORY_DIR)
}

/// Where `session_id`'s history lives, relative to the work directory.
pub fn history_relpath(session_id: &str) -> PathBuf {
    Path::new(HISTORY_DIR).join(format!("{session_id}.terminal"))
}

/// Keep the tail of `captured` that fits [`MAX_HISTORY_BYTES`], cut at a line
/// start so no line is half there, behind a reset so colors from the dropped
/// part do not carry over.
fn bound_history(captured: Vec<u8>) -> Vec<u8> {
    if captured.len() <= MAX_HISTORY_BYTES {
        return captured;
    }
    let room = MAX_HISTORY_BYTES - SGR_RESET.len();
    let tail = &captured[captured.len() - room..];
    let start = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    [SGR_RESET, &tail[start..]].concat()
}

/// Write `history` as `session_id`'s history file, replacing any earlier one.
///
/// The write goes through `safe_fs`, which refuses a symlink at any component
/// of the path.
pub fn write_history(work_dir: &Path, session_id: &str, history: &[u8]) -> Result<()> {
    validate_id(session_id).context("invalid session id for terminal history")?;
    safe_create_dir_all(work_dir, Path::new(HISTORY_DIR), 0o700)?;
    safe_locked_write(work_dir, &history_relpath(session_id), history)
}

/// Capture `session`'s whole scrollback and write it to its history file.
///
/// Best-effort: a pane that cannot be read (its server already gone, a probe
/// timeout) or a file that cannot be written is logged, and the kill the caller
/// is about to perform goes ahead. A capture that yields nothing leaves an
/// earlier history file alone.
pub(super) fn save_history(work_dir: &Path, session: &Session) {
    let Some(captured) = capture_history(session).filter(|bytes| !bytes.is_empty()) else {
        return;
    };
    if let Err(error) = write_history(work_dir, &session.id, &bound_history(captured)) {
        tracing::warn!(
            session_id = %session.id,
            %error,
            "Failed to save terminal history; the session is killed without it"
        );
    }
}

/// Whether `session_id` has a history file. An id that is unsafe to join into a
/// path has none.
pub fn has_history(work_dir: &Path, session_id: &str) -> bool {
    validate_id(session_id).is_ok()
        && matches!(
            open_regular_no_follow(
                work_dir,
                &history_relpath(session_id).to_string_lossy(),
                libc::O_RDONLY,
            ),
            Ok(Some(_))
        )
}

/// The bytes of `session_id`'s history file, `None` when there is none. A file
/// over [`MAX_HISTORY_BYTES`], a symlink or a non-regular file is an error.
pub fn read_history(work_dir: &Path, session_id: &str) -> Result<Option<Vec<u8>>> {
    validate_id(session_id).context("invalid session id for terminal history")?;
    match read_bounded(work_dir, &history_relpath(session_id), MAX_HISTORY_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn history_lives_in_its_own_directory_under_the_session_id() {
        assert_eq!(
            history_relpath("session-ab12cd34-1700000000"),
            Path::new("terminal-history/session-ab12cd34-1700000000.terminal")
        );
        assert_eq!(
            history_dir(Path::new("/repo/.loom/work")),
            Path::new("/repo/.loom/work/terminal-history")
        );
    }

    #[test]
    fn the_history_directory_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let work = TempDir::new().unwrap();

        write_history(work.path(), "session-a", b"x").unwrap();

        let mode = std::fs::metadata(history_dir(work.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn written_history_reads_back_and_is_reported() {
        let work = TempDir::new().unwrap();
        assert!(!has_history(work.path(), "session-a"));
        assert_eq!(read_history(work.path(), "session-a").unwrap(), None);

        write_history(work.path(), "session-a", b"\x1b[31mred\x1b[0m\nline\n").unwrap();

        assert!(has_history(work.path(), "session-a"));
        assert_eq!(
            read_history(work.path(), "session-a").unwrap().as_deref(),
            Some(&b"\x1b[31mred\x1b[0m\nline\n"[..])
        );
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let work = TempDir::new().unwrap();
        write_history(work.path(), "session-a", b"first and longer").unwrap();
        write_history(work.path(), "session-a", b"second").unwrap();

        let bytes = read_history(work.path(), "session-a").unwrap().unwrap();
        assert_eq!(bytes, b"second");
    }

    #[test]
    fn an_id_that_could_leave_the_history_directory_is_refused() {
        let work = TempDir::new().unwrap();
        for id in ["../escape", "a/b", ""] {
            assert!(write_history(work.path(), id, b"x").is_err(), "{id:?}");
            assert!(read_history(work.path(), id).is_err(), "{id:?}");
            assert!(!has_history(work.path(), id), "{id:?}");
        }
    }

    #[test]
    fn a_planted_symlink_is_not_followed_on_write_or_read() {
        let work = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("target");
        std::fs::write(&target, b"untouched").unwrap();
        std::fs::create_dir_all(history_dir(work.path())).unwrap();
        std::os::unix::fs::symlink(&target, work.path().join(history_relpath("session-a")))
            .unwrap();

        assert!(write_history(work.path(), "session-a", b"overwritten").is_err());
        assert!(read_history(work.path(), "session-a").is_err());
        assert!(!has_history(work.path(), "session-a"));
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
    }

    #[test]
    fn history_over_the_limit_keeps_whole_lines_from_the_end() {
        let line = b"0123456789abcdef\n";
        let lines = MAX_HISTORY_BYTES / line.len() + 10;
        let captured: Vec<u8> = line
            .iter()
            .copied()
            .cycle()
            .take(lines * line.len())
            .collect();

        let bounded = bound_history(captured);

        assert!(bounded.len() <= MAX_HISTORY_BYTES);
        assert!(bounded.starts_with(SGR_RESET));
        assert!(bounded[SGR_RESET.len()..].starts_with(b"0123456789abcdef\n"));
        assert!(bounded.ends_with(b"0123456789abcdef\n"));
    }

    #[test]
    fn history_within_the_limit_is_untouched() {
        assert_eq!(bound_history(b"short\n".to_vec()), b"short\n");
    }

    #[test]
    fn server_options_raise_the_history_limit_and_keep_a_dead_pane() {
        let names: Vec<&str> = SERVER_OPTIONS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["history-limit", "remain-on-exit"]);
        let limit: usize = SERVER_OPTIONS[0].1.parse().unwrap();
        assert!(limit > 2000);
        assert_eq!(SERVER_OPTIONS[1].1, "on");
    }
}
