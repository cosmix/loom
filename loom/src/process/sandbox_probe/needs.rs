//! Skip decisions for the capabilities a host job's or session sandbox withholds, one
//! per capability: each returns true when the caller must return, after `skip_unless`
//! printed a `SKIP` line (or panicked under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::daemon::{socket_path_fits, SUN_PATH_MAX};

use super::{loopback_bindable, path_writable, process_tree_visible, skip_unless};

/// Skip when this process cannot bind a loopback TCP listener.
pub fn skip_without_loopback(test_name: &str) -> bool {
    skip_unless(
        loopback_bindable(),
        test_name,
        "this process cannot bind a loopback TCP listener",
    )
}

/// Skip when an `AF_UNIX` listener cannot be bound at `socket` (the path is too long for
/// `sun_path`, or its directory refuses the bind).
pub fn skip_without_unix_socket(test_name: &str, socket: &Path) -> bool {
    if skip_unless(
        socket_path_fits(socket),
        test_name,
        &format!(
            "socket path {} is {} bytes; at most {} fit",
            socket.display(),
            socket.as_os_str().len(),
            SUN_PATH_MAX - 1
        ),
    ) {
        return true;
    }
    // Bind a short-named probe in the nearest existing ancestor: the socket's own
    // directories may not exist yet, and creating them could change what the test checks.
    let ancestor = socket
        .ancestors()
        .skip(1)
        .find(|dir| dir.is_dir())
        .unwrap_or(Path::new("."));
    let probe = ancestor.join(format!(".lsp{}", std::process::id()));
    let bound = std::os::unix::net::UnixListener::bind(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    skip_unless(
        bound,
        test_name,
        "this process cannot bind an AF_UNIX listener",
    )
}

/// Skip when `ps` and the parent-pid chain are hidden, so a hook that proves a live
/// ancestor (`LOOM_MAIN_AGENT_PID`) can never find one.
pub fn skip_without_process_tree(test_name: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        test_name,
        "this process cannot read its process ancestry",
    )
}

/// Skip when a confined command cannot start (a nested Seatbelt profile or bubblewrap is
/// refused). `confined_commands_runnable` prints the reason itself.
pub fn skip_without_confinement() -> bool {
    !super::confined_commands_runnable()
}

/// Whether `bash` can redirect a command's stderr into a process substitution
/// (`cmd 2> >(...)`), which opens `/dev/fd/<n>` for writing; a host job's or a session's
/// Seatbelt profile can refuse that open (`exec 2> >(...)` passes where `cmd 2> >(...)`
/// does not, so the probe uses the command form). Memoized.
pub fn bash_process_substitution_usable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        std::process::Command::new("bash")
            .args(["-c", "true 2> >(cat >/dev/null) && echo probe"])
            .output()
            .is_ok_and(|out| {
                out.status.success()
                    && out.stdout
                        == b"probe
"
            })
    })
}

/// Skip when `bash` cannot run a process substitution, which the session wrapper script
/// uses to tee its stderr.
pub fn skip_without_process_substitution(test_name: &str) -> bool {
    skip_unless(
        bash_process_substitution_usable(),
        test_name,
        "bash cannot open /dev/fd for a process substitution",
    )
}

/// Lockfile names a JavaScript runner looks up along the ancestors of its project.
const LOCKFILES: [&str; 5] = [
    "bun.lock",
    "bun.lockb",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
];

/// The first lockfile among the ancestors of `dir` that exists but is refused to this
/// process (a host job's profile denies reads outside the checkout), if any. A runner that
/// hashes the nearest lockfile at startup dies with `EPERM` on it.
pub fn denied_ancestor_lockfile(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .flat_map(|ancestor| LOCKFILES.map(|name| ancestor.join(name)))
        .find(|path| {
            matches!(std::fs::File::open(path),
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied)
        })
}

/// Skip when a JavaScript runner started under `dir` would be refused a lockfile in one of
/// its ancestors.
pub fn skip_without_lockfile_access(test_name: &str, dir: &Path) -> bool {
    let denied = denied_ancestor_lockfile(dir);
    skip_unless(
        denied.is_none(),
        test_name,
        &format!("reading {denied:?} is refused"),
    )
}

/// The directory `fs::locking` keeps plan locks in: `/tmp/loom-plan-locks-<uid>`, or under
/// the temp dir in a unit-test build of the library.
fn plan_lock_dir() -> PathBuf {
    let base = if cfg!(test) {
        std::env::temp_dir()
    } else {
        PathBuf::from("/tmp")
    };
    // SAFETY: geteuid has no preconditions and does not access caller memory.
    let uid = unsafe { libc::geteuid() };
    base.join(format!("loom-plan-locks-{uid}"))
}

/// Whether a plan lock file can be created: the lock directory is writable, or can be
/// created in its parent. Memoized.
pub fn plan_locks_usable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        let dir = plan_lock_dir();
        if dir.is_dir() {
            path_writable(&dir)
        } else {
            dir.parent().is_some_and(path_writable)
        }
    })
}

/// Skip when plan locks cannot be taken (a read-only `/tmp`), which fails every plan
/// amendment and every plan-file append.
pub fn skip_without_plan_locks(test_name: &str) -> bool {
    skip_unless(
        plan_locks_usable(),
        test_name,
        "the plan lock directory is not writable",
    )
}

/// Returns from the calling test, naming it, when an `AF_UNIX` listener cannot be bound at
/// the given socket path. One line keeps a test under the function size limit.
#[cfg(test)]
macro_rules! require_unix_socket {
    ($test:literal, $socket:expr) => {
        if $crate::process::sandbox_probe::skip_without_unix_socket($test, $socket) {
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use require_unix_socket;

/// Like [`require_unix_socket`], and also returns when no loopback TCP listener can be
/// bound, for a test that serves a socket and connects through a loopback stand-in.
#[cfg(test)]
macro_rules! require_unix_socket_and_loopback {
    ($test:literal, $socket:expr) => {
        if $crate::process::sandbox_probe::skip_without_loopback($test)
            || $crate::process::sandbox_probe::skip_without_unix_socket($test, $socket)
        {
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use require_unix_socket_and_loopback;
