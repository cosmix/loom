//! Where the daemon's control socket lives, and whether that path can be bound.
//!
//! Every client resolves the socket through [`socket_path`]; the daemon's own
//! bind keeps `work_dir.join(SOCKET_FILE)` because its work root is already
//! real.

use std::path::{Path, PathBuf};

/// File name of the daemon's control socket inside the state root.
pub const SOCKET_FILE: &str = "orchestrator.sock";

/// `sockaddr_un.sun_path` limit: 104 bytes on macOS/BSD, 108 on Linux. Budget
/// the tighter macOS/BSD bound on both platforms — the portable choice, and
/// the one `orchestrator/terminal/tmux/tests.rs` documents for the same
/// reason.
pub const SUN_PATH_MAX: usize = 104;

/// Longest repository path (bytes) whose `.loom/work/orchestrator.sock` still
/// fits: the limit, less the NUL terminator, the socket's own suffix and the
/// separator between the repository and `.loom`.
const MAX_REPO_PATH_BYTES: usize = SUN_PATH_MAX - 1 - ".loom/work/".len() - SOCKET_FILE.len() - 1;

/// Whether `path`, plus its NUL terminator, fits `sockaddr_un.sun_path`.
///
/// The kernel stores the pathname NUL-terminated, so the real budget is
/// `len() + 1 <= SUN_PATH_MAX`, i.e. `len() < SUN_PATH_MAX` — a strict `<`,
/// not `<=`. `as_os_str().len()` is already a byte count on Unix (not a char
/// count), so this is correct for a non-ASCII path with no extra work.
pub fn socket_path_fits(path: &Path) -> bool {
    path.as_os_str().len() < SUN_PATH_MAX
}

/// The daemon's socket for `work_dir`.
///
/// In a stage worktree `.loom/work` is a symlink to the state root, and the
/// daemon bound its socket under the resolved, shorter path. The unresolved
/// worktree path can pass the `sun_path` limit and make `connect` fail with
/// `AF_UNIX path too long`, so resolve it first. When `work_dir` cannot be
/// resolved, the given spelling is used as is.
pub fn socket_path(work_dir: &Path) -> PathBuf {
    work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf())
        .join(SOCKET_FILE)
}

/// Why the daemon cannot serve `work_dir`, or `None` when its resolved socket
/// path fits `sun_path`.
pub fn socket_path_problem(work_dir: &Path) -> Option<String> {
    let path = socket_path(work_dir);
    if socket_path_fits(&path) {
        return None;
    }
    Some(format!(
        "daemon socket path '{}' is {} bytes; AF_UNIX paths must be under {SUN_PATH_MAX} bytes. \
         Move the repository to a path of at most {MAX_REPO_PATH_BYTES} bytes.",
        path.display(),
        path.as_os_str().len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A canonical temp root, so the tests hold on platforms whose temp dir
    /// is itself a symlink.
    fn canonical_temp() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    #[test]
    fn socket_path_resolves_a_symlinked_state_root() {
        let (_dir, root) = canonical_temp();
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_eq!(socket_path(&link), real.join(SOCKET_FILE));
    }

    #[test]
    fn an_unresolvable_root_keeps_the_given_spelling() {
        let (_dir, root) = canonical_temp();
        let missing = root.join("missing");

        assert_eq!(socket_path(&missing), missing.join(SOCKET_FILE));
    }

    #[test]
    fn socket_problem_names_bytes_limit_and_path() {
        let (_dir, root) = canonical_temp();
        let long = root.join("d".repeat(SUN_PATH_MAX));
        std::fs::create_dir(&long).unwrap();
        let socket = long.join(SOCKET_FILE);

        let message = socket_path_problem(&long).expect("a long socket path is a problem");

        assert!(message.contains(&socket.as_os_str().len().to_string()));
        assert!(message.contains("104"));
        assert!(message.contains(&socket.display().to_string()));
        assert!(message.contains("at most 74 bytes"));
    }

    #[test]
    fn socket_problem_is_none_for_a_short_root() {
        let (_dir, root) = canonical_temp();
        assert!(
            socket_path_fits(&root.join(SOCKET_FILE)),
            "environment precondition: temp dir too long ({})",
            root.display()
        );

        assert_eq!(socket_path_problem(&root), None);
    }

    #[test]
    fn socket_problem_measures_the_resolved_path() {
        let (_dir, root) = canonical_temp();
        let real = root.join("r");
        std::fs::create_dir(&real).unwrap();
        let long_parent = root.join("l".repeat(SUN_PATH_MAX));
        std::fs::create_dir(&long_parent).unwrap();
        let link = long_parent.join("work");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert!(!socket_path_fits(&link.join(SOCKET_FILE)));
        assert!(
            socket_path_fits(&real.join(SOCKET_FILE)),
            "environment precondition: temp dir too long ({})",
            root.display()
        );

        assert_eq!(socket_path_problem(&link), None);
    }
}
