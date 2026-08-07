//! tmux socket housekeeping.
//!
//! Sockets live in a directory tmux itself owns per-USER (`$TMUX_TMPDIR` or
//! `/tmp`, joined with `tmux-<uid>`), NOT per-repository. That makes the
//! directory shared by every loom checkout this user runs, so anything that
//! walks it must never act on a socket it cannot positively attribute to the
//! caller's own `.work` directory — see [`LoomSocket::attributed`].

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::session::Session;

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

    let resolved = super::native::NativeBackend::window_title_and_pid_key(&session);
    if let Some((_, pid_key)) = &resolved {
        if let Some(entry) = super::native::read_pid_entry(work_dir, pid_key) {
            if super::native::pid_matches_entry(&entry) {
                return true;
            }
        }
    }

    if let Some(pid) = session.pid {
        if crate::process::is_process_alive(pid) {
            return true;
        }
    }

    false
}

/// Best-effort `tmux -S <socket_path> kill-server`. Returns whether the
/// command exited successfully; a socket that no longer exists is not
/// treated specially by the caller (best-effort teardown either way).
pub fn kill_socket_server(socket_path: &Path) -> bool {
    Command::new("tmux")
        .args(["-S", &socket_path.to_string_lossy(), "kill-server"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Points `TMUX_TMPDIR` at an isolated directory for the duration of the
    /// test and restores it on drop — `loom_socket_dir()` honors the env var,
    /// so this makes `list_loom_sockets` deterministic without a real tmux
    /// server. `#[serial]` on callers avoids racing other TMUX_TMPDIR tests.
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
    fn socket_session_is_alive_false_for_missing_session_file() {
        let work_dir = TempDir::new().unwrap();
        assert!(!socket_session_is_alive(
            work_dir.path(),
            "session-missing-0000000000"
        ));
    }
}
