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

/// Name of the gitignore-syntax file the daemon publishes at the state root so
/// `rg`, `fd` and `ag` skip the credential files.
pub(crate) const SEARCH_IGNORE_FILE: &str = ".ignore";
/// Name of the ripgrep config the daemon publishes at the state root; the
/// session wrapper exports it as `RIPGREP_CONFIG_PATH`.
pub(crate) const RIPGREP_CONFIG_FILE: &str = "ripgreprc";

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

/// Body of the [`SEARCH_IGNORE_FILE`] published at the state root. A
/// sandboxed agent's `rg`/`fd`/`ag` opening `admin.token` or `user.token`
/// hits the sandbox's own deny rule and stalls auto mode on an operator
/// prompt, so the daemon excludes both from ordinary directory sweeps.
/// Generated from [`STATE_ROOT_TOKEN_FILES`] so the excluded names can never
/// drift from the sandbox deny rules.
pub(crate) fn search_ignore_body() -> String {
    let mut body = String::from(
        "# Written by the loom daemon. Keeps rg, fd and ag away from the daemon\n\
         # credential files: a sandboxed agent that opens one triggers an operator prompt.\n",
    );
    for name in STATE_ROOT_TOKEN_FILES {
        body.push_str(name);
        body.push('\n');
    }
    body
}

/// Body of the [`RIPGREP_CONFIG_FILE`] published at the state root, exported
/// to sessions as `RIPGREP_CONFIG_PATH` so the exclusion survives a
/// `-uu`/`--no-ignore` sweep that would otherwise bypass
/// [`search_ignore_body`]. Generated from [`STATE_ROOT_TOKEN_FILES`] for the
/// same reason.
pub(crate) fn ripgrep_config_body() -> String {
    let mut body = String::from(
        "# Written by the loom daemon; exported to agent sessions as RIPGREP_CONFIG_PATH.\n\
         # Excludes the daemon credential files even from --no-ignore / -uu sweeps.\n",
    );
    for name in STATE_ROOT_TOKEN_FILES {
        body.push_str("--glob=!");
        body.push_str(name);
        body.push('\n');
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_ignore_body_lists_token_files() {
        let body = search_ignore_body();
        assert!(body.starts_with('#'));
        assert!(body.lines().any(|line| line == "admin.token"));
        assert!(body.lines().any(|line| line == "user.token"));
    }

    #[test]
    fn ripgrep_config_body_excludes_token_files() {
        let body = ripgrep_config_body();
        assert!(body.starts_with('#'));
        assert!(body.lines().any(|line| line == "--glob=!admin.token"));
        assert!(body.lines().any(|line| line == "--glob=!user.token"));
    }
}
