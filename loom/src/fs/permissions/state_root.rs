//! Shared resolution of a worktree's state-root symlink and the S-1 token
//! deny-list every settings writer must attach to it.
//!
//! In a worktree, the state root is a symlink — `.loom/work` on the nested
//! layout (-> `../../../.loom/work`) or `.work` on a legacy workspace
//! (-> `../../.work`) — pointing at the main repo's shared state. Claude
//! Code resolves symlinks before checking permission patterns, so a
//! worktree-relative `Read(.loom/work/**)` pattern never matches the
//! resolved absolute path; callers add the resolved path explicitly.
//!
//! SECURITY (S-1): a blanket `Read(/{resolved}/**)` (or `Edit(/{resolved}/**)`)
//! over that resolved path exposes `admin.token` (Admin RPC capability) and
//! `user.token` (User capability) to a sandboxed worktree agent — a daemon
//! RPC privilege escalation. Both settings writers (`.claude/settings.json`
//! in `git::worktree::settings` and `.claude/settings.local.json` in
//! `sandbox::settings`) instead emit explicit `deny` rules for the
//! resolved-absolute token paths *before* any narrower allow, so deny wins
//! over any current or future allow that might match the state root. This
//! module is the one place those token paths are named, so a third token is
//! added in exactly one place and both writers pick it up.

use std::path::{Path, PathBuf};

/// Token files a sandboxed worktree agent must never be granted a blanket
/// `Read` over (S-1). Add a new one here — nowhere else.
pub(crate) const STATE_ROOT_TOKEN_FILES: [&str; 2] = ["admin.token", "user.token"];

/// Resolve a worktree's state-root symlink to its canonical absolute target.
///
/// Prefers the nested layout (`.loom/work`) when it exists or is a symlink,
/// otherwise falls back to the legacy `.work`. Returns `None` when neither
/// spelling exists as a symlink (or directory) or when canonicalization
/// fails, so callers can skip adding state-root permissions entirely.
pub(crate) fn resolve_state_root(worktree_path: &Path) -> Option<PathBuf> {
    let nested = worktree_path.join(".loom").join("work");
    let work_link = if nested.exists() || nested.is_symlink() {
        nested
    } else {
        worktree_path.join(".work")
    };

    if !work_link.exists() && !work_link.is_symlink() {
        return None;
    }

    work_link.canonicalize().ok()
}

/// Build the two `Read(/{resolved}/{token})` deny-permission strings for the
/// S-1 guard (see module docs).
///
/// `resolved` is the caller's own string form of the canonical state-root
/// path, so extracting this does not shift either call site's existing
/// UTF-8 handling (one hard-errors on non-UTF8, the other uses a lossy
/// conversion).
pub(crate) fn token_read_denies(resolved: &str) -> [String; 2] {
    let [a, b] = STATE_ROOT_TOKEN_FILES;
    [
        format!("Read(/{resolved}/{a})"),
        format!("Read(/{resolved}/{b})"),
    ]
}

/// Build the two plain absolute token paths (`/{resolved}/{token}`), for
/// call sites that need the bare path rather than a `Read(...)` permission
/// string, e.g. `sandbox.filesystem.denyRead`.
pub(crate) fn token_deny_paths(resolved: &str) -> [String; 2] {
    let [a, b] = STATE_ROOT_TOKEN_FILES;
    [format!("/{resolved}/{a}"), format!("/{resolved}/{b}")]
}
