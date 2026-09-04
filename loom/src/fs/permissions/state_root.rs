//! Shared resolution of a worktree's state-root symlink and the S-1 token
//! paths every settings writer must protect on it.
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
//! `sandbox::settings`) therefore grant only narrow allows over the state
//! root, never a blanket one. This module is the one place the token paths
//! are named, so a third token is added in exactly one place and both writers
//! pick it up.
//!
//! Loom writes NO `Read(...)` entry under `permissions.deny`, in any shape.
//! Claude Code's Bash path validator refuses `rg`, `grep`, `egrep`, `fgrep`,
//! `diff`, `git`, `cp` and `mv` on a relative path issued after a `cd` in the
//! same compound command whenever ANY settings file carries ANY `Read(` deny
//! rule — bypass-immune, not classifier-approvable, and independent of the
//! rule's path shape, so no spelling avoids it. The tokens are instead denied
//! to Bash by the OS-level `sandbox.filesystem.denyRead` list
//! ([`token_deny_paths`]), which is not a permission rule and does not feed
//! that check, and to the native file tools by the `hooks/credential-guard.sh`
//! PreToolUse hook. The recognisers here ([`is_token_read_deny`],
//! [`is_loom_written_read_deny`]) exist only to strip the deny rules older
//! loom versions wrote.

use std::path::{Path, PathBuf};

/// Token files a sandboxed worktree agent must never be granted a blanket
/// `Read` over (S-1). Add a new one here — nowhere else.
pub(crate) const STATE_ROOT_TOKEN_FILES: [&str; 2] = ["admin.token", "user.token"];

/// Credential paths loom's own sandbox denies at the OS level
/// (`sandbox.filesystem.denyRead`). Loom never mirrors any of them into
/// `permissions.deny` as a `Read(...)` rule - see the module docs - so this
/// is also the list [`is_loom_written_read_deny`] uses to recognise the
/// mirrors older loom versions did write. Named once here so
/// `models::stage::types::default_deny_read`, `sandbox::settings::policy`'s
/// mandatory list, and that recogniser cannot drift apart.
pub(crate) const CREDENTIAL_DENY_READ_PATHS: [&str; 5] = [
    "~/.ssh/**",
    "~/.aws/**",
    "~/.config/gcloud/**",
    "~/.gnupg/**",
    "~/.claude/.credentials.json",
];

/// State-root layout a project's `config.toml` was found under: the current
/// nested spelling, and the legacy workspace one a project keeps forever once
/// it started on it.
const NESTED_LAYOUT: &str = ".loom/work";
const LEGACY_LAYOUT: &str = ".work";

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

/// Strip a `Read(...)` permission rule down to the path inside it.
fn read_rule_path(entry: &str) -> Option<&str> {
    entry.strip_prefix("Read(")?.strip_suffix(')')
}

/// Whether a `permissions.deny` entry is a `Read(...)` rule naming a token
/// file under a known state-root layout — every spelling loom has ever
/// emitted: resolved absolute, parent-glob, and the worktree-relative forms
/// with or without `../` prefixes.
pub(crate) fn is_token_read_deny(entry: &str) -> bool {
    let Some(path) = read_rule_path(entry) else {
        return false;
    };
    let Some(dir) = STATE_ROOT_TOKEN_FILES
        .into_iter()
        .find_map(|token| path.strip_suffix(token)?.strip_suffix('/'))
    else {
        return false;
    };
    [NESTED_LAYOUT, LEGACY_LAYOUT]
        .into_iter()
        .any(|layout| dir == layout || dir.ends_with(&format!("/{layout}")))
}

/// A `permissions.deny` entry loom itself wrote in some earlier version:
/// a daemon token deny in any spelling, or a `Read(...)` mirror of one of
/// the credential paths the OS sandbox denies. Everything else in a deny
/// list is the operator's and is never removed by loom.
pub(crate) fn is_loom_written_read_deny(entry: &str) -> bool {
    if is_token_read_deny(entry) {
        return true;
    }
    read_rule_path(entry).is_some_and(|path| CREDENTIAL_DENY_READ_PATHS.contains(&path.trim()))
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
    fn every_token_deny_spelling_loom_ever_wrote_is_still_recognized() {
        for entry in [
            // resolved-absolute, both layouts
            "Read(//home/you/src/app/.work/admin.token)",
            "Read(//home/you/src/app/.loom/work/user.token)",
            // worktree-relative, with and without the `../` prefix
            "Read(.work/admin.token)",
            "Read(.loom/work/user.token)",
            "Read(../.work/user.token)",
            "Read(../.loom/work/admin.token)",
            // the parent-glob shape the last version emitted
            "Read(//home/you/src/*/.loom/work/admin.token)",
            "Read(//home/you/src/*/.work/user.token)",
        ] {
            assert!(is_token_read_deny(entry), "{entry} must be recognized");
            assert!(is_loom_written_read_deny(entry), "{entry} is loom's own");
        }
    }

    #[test]
    fn loom_written_denies_cover_the_credential_mirrors_and_nothing_of_the_operators() {
        for path in CREDENTIAL_DENY_READ_PATHS {
            let entry = format!("Read({path})");
            assert!(
                is_loom_written_read_deny(&entry),
                "{entry} is a mirror loom used to write"
            );
        }
        assert!(is_loom_written_read_deny(
            "Read(//home/you/src/app/.loom/work/admin.token)"
        ));

        for entry in [
            // the operator's own rules
            "Read(secrets/**)",
            // no glob, so not one of ours
            "Read(~/.ssh)",
            // not a Read rule at all
            "Edit(~/.ssh/**)",
        ] {
            assert!(
                !is_loom_written_read_deny(entry),
                "{entry} must not be claimed as loom's"
            );
        }
    }

    #[test]
    fn unrelated_deny_entries_are_not_token_denies() {
        for entry in [
            "Read(~/.ssh/**)",
            "Edit(//home/you/src/app/.work/admin.token)",
            "Read(//home/you/src/app/.work/signals/**)",
        ] {
            assert!(!is_token_read_deny(entry), "{entry} must not be claimed");
        }
    }

    #[test]
    fn ripgrep_config_body_excludes_token_files() {
        let body = ripgrep_config_body();
        assert!(body.starts_with('#'));
        assert!(body.lines().any(|line| line == "--glob=!admin.token"));
        assert!(body.lines().any(|line| line == "--glob=!user.token"));
    }
}
