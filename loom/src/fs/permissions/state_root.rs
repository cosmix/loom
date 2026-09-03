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
//! `sandbox::settings`) instead emit explicit `deny` rules naming the token
//! files *before* any narrower allow, so deny wins over any current or future
//! allow that might match the state root. This module is the one place those
//! token paths are named, so a third token is added in exactly one place and
//! both writers pick it up.
//!
//! Those deny rules are spelled with the project directory globbed out —
//! `Read(//home/you/src/*/.work/admin.token)`, not the resolved-absolute
//! path. Claude Code's `deniedPathInsideDirectory` check refuses `rg`, `grep`,
//! `diff`, `git`, `cp` and `mv` over any directory that contains a `Read(...)`
//! deny's wildcard-free prefix, so a rule whose prefix lies under the project
//! root makes every search from the project root prompt the operator. Putting
//! the `*` where the project name goes moves that prefix up to the project's
//! parent, which no in-project search covers. A `**` anchored at `/` or `~`
//! would move it further still, but every deny rule is also fed to the Linux
//! sandbox, which expands wildcards against the real filesystem — so exactly
//! one `*` keeps that expansion to a single directory listing.

use std::path::{Path, PathBuf};

/// Token files a sandboxed worktree agent must never be granted a blanket
/// `Read` over (S-1). Add a new one here — nowhere else.
pub(crate) const STATE_ROOT_TOKEN_FILES: [&str; 2] = ["admin.token", "user.token"];

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

/// Build the two token deny-permission strings for the S-1 guard, with the
/// project directory replaced by a single `*` so the rule's wildcard-free
/// prefix lands on the project's PARENT (see module docs for why):
/// `/home/you/src/app/.work` yields `Read(//home/you/src/*/.work/admin.token)`.
///
/// A `resolved` path ending in neither known layout keeps the concrete
/// `Read(/{resolved}/{token})` spelling — a shape this module cannot take
/// apart must still be denied.
///
/// `resolved` is the caller's own string form of the canonical state-root
/// path, so extracting this does not shift either call site's existing
/// UTF-8 handling (one hard-errors on non-UTF8, the other uses a lossy
/// conversion).
pub(crate) fn token_read_denies(resolved: &str) -> [String; 2] {
    let [a, b] = STATE_ROOT_TOKEN_FILES;
    let Some((project_root, layout)) = split_state_root(Path::new(resolved)) else {
        return [
            format!("Read(/{resolved}/{a})"),
            format!("Read(/{resolved}/{b})"),
        ];
    };
    let any_sibling_project = project_root
        .parent()
        .unwrap_or_else(|| Path::new("/"))
        .join("*")
        .join(layout);
    [
        format!("Read(/{})", any_sibling_project.join(a).display()),
        format!("Read(/{})", any_sibling_project.join(b).display()),
    ]
}

/// Split a resolved state root into its project root and layout, e.g.
/// `/srv/app/.loom/work` into `/srv/app` and `.loom/work`. `None` when the
/// path ends in neither layout.
fn split_state_root(resolved: &Path) -> Option<(PathBuf, &'static str)> {
    let name = resolved.file_name()?.to_str()?;
    let parent = resolved.parent()?;
    if name == LEGACY_LAYOUT {
        return Some((parent.to_path_buf(), LEGACY_LAYOUT));
    }
    if name == "work" && parent.file_name().and_then(|n| n.to_str()) == Some(".loom") {
        return Some((parent.parent()?.to_path_buf(), NESTED_LAYOUT));
    }
    None
}

/// Whether a bare path (not a `Read(...)` rule) names one of the token files.
pub(crate) fn names_a_token_file(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| STATE_ROOT_TOKEN_FILES.contains(&name))
}

/// Strip a `Read(...)` permission rule down to the path inside it.
fn read_rule_path(entry: &str) -> Option<&str> {
    entry.strip_prefix("Read(")?.strip_suffix(')')
}

/// Split a `Read(...)` rule that names a token file under a known state-root
/// layout into that rule's directory and the layout it ends with. `None` for
/// every other rule. Covers all spellings loom has ever emitted: resolved
/// absolute, parent-glob, and the worktree-relative forms with or without
/// `../` prefixes.
fn split_token_deny(entry: &str) -> Option<(&str, &'static str)> {
    let path = read_rule_path(entry)?;
    let dir = STATE_ROOT_TOKEN_FILES
        .into_iter()
        .find_map(|token| path.strip_suffix(token)?.strip_suffix('/'))?;
    let layout = [NESTED_LAYOUT, LEGACY_LAYOUT]
        .into_iter()
        .find(|layout| dir == *layout || dir.ends_with(&format!("/{layout}")))?;
    Some((dir, layout))
}

/// Whether a `permissions.deny` entry names one of the token files, in any
/// spelling loom has ever written.
pub(crate) fn is_token_read_deny(entry: &str) -> bool {
    split_token_deny(entry).is_some()
}

/// Whether a token deny is in the parent-glob spelling this module now emits.
/// An entry `is_token_read_deny` accepts and this rejects is a stale shape
/// that makes every `rg`/`grep` from the project root prompt the operator.
pub(crate) fn is_parent_glob_token_deny(entry: &str) -> bool {
    let Some((dir, layout)) = split_token_deny(entry) else {
        return false;
    };
    dir.starts_with("//") && dir.strip_suffix(layout).is_some_and(|p| p.ends_with("/*/"))
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
    fn token_denies_glob_the_project_directory_on_both_layouts() {
        assert_eq!(
            token_read_denies("/home/you/src/app/.work"),
            [
                "Read(//home/you/src/*/.work/admin.token)".to_string(),
                "Read(//home/you/src/*/.work/user.token)".to_string(),
            ]
        );
        assert_eq!(
            token_read_denies("/home/you/src/app/.loom/work"),
            [
                "Read(//home/you/src/*/.loom/work/admin.token)".to_string(),
                "Read(//home/you/src/*/.loom/work/user.token)".to_string(),
            ]
        );
    }

    #[test]
    fn a_project_at_the_filesystem_root_gets_no_triple_slash() {
        assert_eq!(
            token_read_denies("/app/.work")[0],
            "Read(//*/.work/admin.token)"
        );
    }

    #[test]
    fn an_unknown_layout_keeps_the_concrete_paths() {
        assert_eq!(
            token_read_denies("/var/lib/loom-state"),
            [
                "Read(//var/lib/loom-state/admin.token)".to_string(),
                "Read(//var/lib/loom-state/user.token)".to_string(),
            ]
        );
    }

    #[test]
    fn every_token_deny_spelling_is_recognized_and_only_the_glob_is_current() {
        let stale = [
            "Read(//home/you/src/app/.work/admin.token)",
            "Read(//home/you/src/app/.loom/work/user.token)",
            "Read(.work/admin.token)",
            "Read(.loom/work/user.token)",
            "Read(../.work/user.token)",
            "Read(../.loom/work/admin.token)",
        ];
        for entry in stale {
            assert!(is_token_read_deny(entry), "{entry} must be recognized");
            assert!(
                !is_parent_glob_token_deny(entry),
                "{entry} is the shape that prompts, not the current one"
            );
        }

        for entry in token_read_denies("/home/you/src/app/.loom/work") {
            assert!(is_token_read_deny(&entry));
            assert!(is_parent_glob_token_deny(&entry));
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
            assert!(!is_parent_glob_token_deny(entry));
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
