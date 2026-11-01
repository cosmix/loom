use super::*;
use crate::fs::session_files::session_to_markdown;
use crate::models::session::Session;

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

fn live_sockets(repo_root: &Path, socket_dir: &Path) -> (PathBuf, PathBuf) {
    let sessions_dir = repo_root.join(".loom").join("work").join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();

    let mut live_session = Session::new();
    live_session.id = "session-livecase00".to_string();
    live_session.assign_to_stage("stage-livecase00".to_string());
    live_session.pid = Some(std::process::id());
    fs::write(
        sessions_dir.join(format!("{}.md", live_session.id)),
        session_to_markdown(&live_session),
    )
    .unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(
        &repo_root.join(".loom").join("work"),
        &live_session,
        std::process::id(),
    )
    .unwrap();
    let live_socket = socket_dir.join(format!("loom-{}", live_session.id));
    fs::write(&live_socket, "").unwrap();

    let unattributed_socket = socket_dir.join("loom-session-strangercase0");
    fs::write(&unattributed_socket, "").unwrap();

    (live_socket, unattributed_socket)
}

/// A fake socket exercises the live kill failure and attribution policy.
#[test]
#[serial]
fn test_cleanup_orphaned_sessions_preserves_live_socket_on_kill_failure() {
    use super::super::cleanup::{cleanup_orphaned_sessions, SessionReapMode};

    let tmux_tmpdir = TempDir::new().unwrap();
    let _guard = TmuxTmpDirGuard::set(tmux_tmpdir.path());
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let socket_dir = tmux_tmpdir.path().join(format!("tmux-{uid}"));
    fs::create_dir_all(&socket_dir).unwrap();

    let repo_root = TempDir::new().unwrap();
    let (live_socket, unattributed_socket) = live_sockets(repo_root.path(), &socket_dir);

    cleanup_orphaned_sessions(repo_root.path(), SessionReapMode::OrphansOnly).unwrap();
    assert!(
        live_socket.exists(),
        "a live attributed session must never be reaped outside --clean"
    );
    assert!(
        unattributed_socket.exists(),
        "an unattributed socket must never be touched"
    );

    assert!(
        cleanup_orphaned_sessions(repo_root.path(), SessionReapMode::IncludeLiveBeforeClean)
            .is_err()
    );
    assert!(
        live_socket.exists(),
        "failed live kill must retain the socket"
    );
    assert!(
        unattributed_socket.exists(),
        "an unattributed socket must never be touched, even in clean mode"
    );
}
