//! The `sockaddr_un.sun_path` length budget checked before the daemon binds
//! its control socket (`lifecycle::run_server`).

use std::path::Path;

/// `sockaddr_un.sun_path` limit: 104 bytes on macOS/BSD, 108 on Linux. Budget
/// the tighter macOS/BSD bound on both platforms — the portable choice, and
/// the one `orchestrator/terminal/tmux/tests.rs` documents for the same
/// reason.
pub(super) const SUN_PATH_MAX: usize = 104;

/// Whether `path`, plus its NUL terminator, fits `sockaddr_un.sun_path`.
///
/// The kernel stores the pathname NUL-terminated, so the real budget is
/// `len() + 1 <= SUN_PATH_MAX`, i.e. `len() < SUN_PATH_MAX` — a strict `<`,
/// not `<=`. `as_os_str().len()` is already a byte count on Unix (not a char
/// count), so this is correct for a non-ASCII path with no extra work.
pub(super) fn socket_path_fits(path: &Path) -> bool {
    path.as_os_str().len() < SUN_PATH_MAX
}
