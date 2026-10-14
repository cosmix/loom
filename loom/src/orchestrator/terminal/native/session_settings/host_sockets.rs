//! The host facts `sandbox::hidden_socket_paths` reads, resolved from the
//! daemon's own process for a session whose stage is granted every Unix
//! socket.

use std::path::{Path, PathBuf};

use super::CapsuleRequest;
use crate::sandbox::SocketHostFacts;

/// The `denyRead` entries that hide the host's Unix sockets from the session
/// `request` launches in `cwd`: none unless the host is Linux and the stage
/// grants every Unix socket. Only Linux's sandbox applies a `denyRead` path by
/// mounting over it, which is what keeps `connect` from reaching the socket.
/// `work_dir` is the state root, whose daemon socket is hidden.
pub(super) fn hidden_sockets(
    request: &CapsuleRequest<'_>,
    cwd: &Path,
    repo_root: &Path,
    work_dir: &Path,
) -> Vec<String> {
    if !cfg!(target_os = "linux") || !crate::sandbox::grants_all_unix_sockets(request.sandbox) {
        return Vec::new();
    }
    let facts = host_facts(request.socket_scan_root, cwd, repo_root, work_dir);
    crate::sandbox::hidden_socket_paths(&facts)
}

fn host_facts(tmp_root: &Path, cwd: &Path, repo_root: &Path, work_dir: &Path) -> SocketHostFacts {
    let signing = crate::git::signing::current();
    let env_path = |name: &str| std::env::var_os(name).map(PathBuf::from);
    SocketHostFacts {
        // SAFETY: `getuid` has no preconditions and cannot fail.
        uid: unsafe { libc::getuid() },
        tmp_root: tmp_root.to_path_buf(),
        xdg_runtime_dir: env_path("XDG_RUNTIME_DIR"),
        ssh_auth_sock: signing.ssh_auth_sock.map(PathBuf::from),
        gnupghome: signing.gnupghome.map(PathBuf::from),
        home: dirs::home_dir(),
        tmux_tmpdir: env_path("TMUX_TMPDIR"),
        loom_tmux_dir: crate::orchestrator::terminal::tmux::loom_socket_dir(),
        daemon_socket: Some(crate::daemon::socket_path(work_dir)),
        tmp_sockets: crate::sandbox::scan_sockets(
            tmp_root,
            crate::sandbox::TMP_SCAN_DEPTH,
            crate::sandbox::TMP_SCAN_MAX_ENTRIES,
        ),
        keep_visible: vec![cwd.to_path_buf(), repo_root.to_path_buf()],
    }
}
