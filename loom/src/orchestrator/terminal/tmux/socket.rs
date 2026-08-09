//! tmux socket housekeeping.
//!
//! Sockets live in a directory tmux itself owns per-USER (`$TMUX_TMPDIR` or
//! `/tmp`, joined with `tmux-<uid>`), NOT per-repository. That makes the
//! directory shared by every loom checkout this user runs, so anything that
//! walks it must never act on a socket it cannot positively attribute to the
//! caller's own `.work` directory — see [`LoomSocket::attributed`].

use std::path::{Path, PathBuf};

use crate::models::session::Session;

use super::native::pid_only_is_alive;

/// The tmux socket directory tmux itself would use: `$TMUX_TMPDIR` if set,
/// else `/tmp`, joined with `tmux-<uid>`.
///
/// Deliberately `/tmp`, not [`std::env::temp_dir`]: on macOS the latter
/// resolves to a per-process `$TMPDIR` under `/var/folders/...`, which is NOT
/// where a real tmux server puts its socket unless `TMUX_TMPDIR` overrides
/// it. This must match tmux's own convention or every lookup here misses.
pub(crate) fn loom_socket_dir() -> PathBuf {
    let base = std::env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    // SAFETY: getuid() is always safe to call and cannot fail.
    let uid = unsafe { libc::getuid() };
    base.join(format!("tmux-{uid}"))
}

/// The on-disk path of a socket named `socket_name` (as returned by
/// [`super::socket_name`]).
pub fn socket_path_for(socket_name: &str) -> PathBuf {
    loom_socket_dir().join(socket_name)
}

/// A loom-owned tmux socket discovered in the per-USER socket directory.
pub struct LoomSocket {
    pub path: PathBuf,
    /// Session id decoded from the socket file name (`loom-<session-id>`).
    pub session_id: String,
    /// True only when `<work_dir>/sessions/<session_id>.md` exists — i.e. the
    /// socket positively belongs to THIS work dir. Callers must never act
    /// destructively on a socket that is not `attributed`.
    pub attributed: bool,
}

/// List every `loom-*` socket found in the shared per-USER tmux socket
/// directory, honestly marking which ones belong to `work_dir`.
///
/// Never deletes anything itself — attribution is a read-only judgment left
/// to the caller, which must skip any socket that is not `attributed` before
/// taking a destructive action (see module docs).
pub fn list_loom_sockets(work_dir: &Path) -> Vec<LoomSocket> {
    let dir = loom_socket_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut sockets = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let Some(session_id) = name.strip_prefix("loom-") else {
            continue;
        };
        // The per-repo overview viewer socket (see
        // `commands::attach::viewer_socket_name`, `loom-view-<8 hex>`) is not
        // a session socket at all, so it can never be session-attributed.
        // Without this skip it decodes to session id `view-<hex>`, which
        // never has a session file, so `loom clean`/`loom init` would report
        // it as an "unattributable tmux socket left untouched" — a warning
        // meant for another checkout's live session, not loom's own viewer.
        // Nothing currently reaps this socket; that is a known limitation.
        if session_id.starts_with("view-") {
            continue;
        }
        let attributed = work_dir
            .join("sessions")
            .join(format!("{session_id}.md"))
            .exists();
        sockets.push(LoomSocket {
            path: entry.path(),
            session_id: session_id.to_string(),
            attributed,
        });
    }
    sockets
}

/// Whether the session recorded at `<work_dir>/sessions/<session_id>.md` is
/// still alive, using the SAME PID layers as
/// [`super::TmuxBackend::is_session_alive`] — but as a free function that
/// needs neither a `TmuxBackend` nor a working terminal. `loom clean` runs
/// headless, where terminal detection can fail; this must not depend on it.
///
/// Returns `false` only when the session file is ABSENT (nothing to be alive)
/// or the PID evidence positively says dead.
///
/// A session file that EXISTS but cannot be read or parsed reports `true`.
/// This is deliberately fail-safe: callers reap on `attributed && !alive`, and
/// `attributed` already means the file exists, so returning `false` here would
/// authorize killing a LIVE session of this very repo whose file merely
/// happened to be mid-write or momentarily unreadable. Never destroy on
/// evidence you could not actually read.
///
/// The PID identity service is shared with the backends and is side-effect
/// free: this probe never deletes the evidence it just reported on.
pub fn socket_session_is_alive(work_dir: &Path, session_id: &str) -> bool {
    let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));
    if !session_path.exists() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&session_path) else {
        return true;
    };
    let Ok(session) =
        crate::parser::frontmatter::parse_from_markdown::<Session>(&content, "Session")
    else {
        return true;
    };

    pid_only_is_alive(work_dir, &session)
}

/// Best-effort `tmux -S <socket_path> kill-server`. Returns whether the
/// command exited successfully; a socket that no longer exists is not
/// treated specially by the caller (best-effort teardown either way).
pub fn kill_socket_server(socket_path: &Path) -> bool {
    super::run_tmux_control(
        &["-S", &socket_path.to_string_lossy(), "kill-server"],
        super::TMUX_TEARDOWN_TIMEOUT,
        format!("tmux kill-server ({})", socket_path.display()),
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Points `TMUX_TMPDIR` at an isolated directory for the duration of the
    /// test and restores it on drop — `loom_socket_dir()` honors the env var,
    /// so this makes `list_loom_sockets` deterministic without a real tmux
    /// server. `#[serial]` on callers avoids racing other TMUX_TMPDIR tests.
    ///
    /// `pub(crate)` because `tmux::tests` needs the same pin: any test that
    /// touches `socket_path_for` is reading the same process-global env var
    /// these tests write, so a second private copy of this guard would be one
    /// more thing to keep in sync for no benefit.
    pub(crate) struct TmuxTmpDirGuard {
        original: Option<std::ffi::OsString>,
    }

    impl TmuxTmpDirGuard {
        pub(crate) fn set(dir: &Path) -> Self {
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

    #[test]
    fn unreadable_session_file_reports_alive_so_it_is_never_reaped() {
        // Fail-safe: callers reap on `attributed && !alive`, and `attributed`
        // already means the file exists. If an unparseable file reported
        // "dead", `loom clean`/`loom init` would kill a LIVE session of this
        // repo whose file was merely mid-write.
        let work = TempDir::new().unwrap();
        std::fs::create_dir_all(work.path().join("sessions")).unwrap();
        std::fs::write(
            work.path().join("sessions").join("session-garbage.md"),
            "this is not valid session frontmatter",
        )
        .unwrap();

        assert!(
            socket_session_is_alive(work.path(), "session-garbage"),
            "an unparseable session file must not be treated as a dead session"
        );
    }

    #[test]
    #[serial]
    fn two_work_dirs_never_see_each_others_sockets_as_attributed() {
        let socket_dir = TempDir::new().unwrap();
        let _guard = TmuxTmpDirGuard::set(socket_dir.path());
        let tmux_socket_dir = loom_socket_dir();
        std::fs::create_dir_all(&tmux_socket_dir).unwrap();

        // One socket on disk, for a session recorded in `owner_work_dir` only.
        let session_id = "session-abcd1234-1111111111";
        std::fs::write(tmux_socket_dir.join(format!("loom-{session_id}")), "").unwrap();

        let owner_work_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(owner_work_dir.path().join("sessions")).unwrap();
        std::fs::write(
            owner_work_dir
                .path()
                .join("sessions")
                .join(format!("{session_id}.md")),
            "---\n---\n",
        )
        .unwrap();

        let stranger_work_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(stranger_work_dir.path().join("sessions")).unwrap();

        let owner_sockets = list_loom_sockets(owner_work_dir.path());
        let stranger_sockets = list_loom_sockets(stranger_work_dir.path());

        assert_eq!(owner_sockets.len(), 1);
        assert!(
            owner_sockets[0].attributed,
            "the work dir that recorded the session must attribute its socket"
        );
        assert_eq!(stranger_sockets.len(), 1);
        assert!(
            !stranger_sockets[0].attributed,
            "an unrelated work dir must never attribute another work dir's socket"
        );
    }

    #[test]
    #[serial]
    fn list_loom_sockets_ignores_the_overview_viewer_socket() {
        let socket_dir = TempDir::new().unwrap();
        let _guard = TmuxTmpDirGuard::set(socket_dir.path());
        let tmux_socket_dir = loom_socket_dir();
        std::fs::create_dir_all(&tmux_socket_dir).unwrap();

        // This repo's own overview viewer socket, named by
        // `commands::attach::viewer_socket_name`. It must never show up as an
        // unattributable session socket.
        std::fs::write(tmux_socket_dir.join("loom-view-deadbeef"), "").unwrap();

        // POSITIVE CONTROL. Asserting only "nothing came back" cannot tell
        // "the viewer socket was skipped" apart from "listing is broken and
        // returns nothing at all" — a `continue` moved one line too far, or a
        // wrong socket dir, would pass the empty assertion happily. A real
        // session socket sitting in the same directory forces the test to
        // prove the skip is SELECTIVE.
        let real_session_id = "session-abcd1234-1111111111";
        std::fs::write(tmux_socket_dir.join(format!("loom-{real_session_id}")), "").unwrap();

        let work_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(work_dir.path().join("sessions")).unwrap();

        let sockets = list_loom_sockets(work_dir.path());

        assert_eq!(
            sockets.len(),
            1,
            "exactly the real session socket must come back, not the viewer socket"
        );
        assert_eq!(sockets[0].session_id, real_session_id);
    }

    #[test]
    fn socket_session_is_alive_never_deletes_the_evidence_it_reports_on() {
        // `loom clean` / `loom init` reap on `attributed && !alive`, so this
        // probe runs BEFORE a destructive decision, on files the caller may
        // still need. Deleting the PID file mid-judgment would erase the
        // identity evidence the destructive caller relies on.
        let work = TempDir::new().unwrap();
        let mut session = crate::models::session::Session::new();
        session.assign_to_stage("reaped-stage".to_string());
        session.pid = Some(999_999_999);
        crate::fs::session_files::save_session(&session, work.path()).unwrap();

        let (_, pid_key) =
            super::super::native::NativeBackend::window_title_and_pid_key(&session).unwrap();
        std::fs::create_dir_all(work.path().join("pids")).unwrap();
        let pid_file = work.path().join("pids").join(format!("{pid_key}.pid"));
        std::fs::write(&pid_file, "999999999\n").unwrap();

        assert!(
            !socket_session_is_alive(work.path(), &session.id),
            "a dead PID with a dead PID file must report dead"
        );
        assert!(
            pid_file.exists(),
            "the probe must be side-effect free — it must not reap the PID file it just read"
        );
    }

    #[test]
    fn socket_session_is_alive_false_for_missing_session_file() {
        let work_dir = TempDir::new().unwrap();
        assert!(!socket_session_is_alive(
            work_dir.path(),
            "session-missing-0000000000"
        ));
    }
}
