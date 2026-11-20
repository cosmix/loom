//! The stage cache as the stage's Claude session sees it: the exports the
//! session wrapper adds to its `exec env -i` line and the sandbox write grant
//! for the root. Seeds, `GOPROXY` and `GOFLAGS` come from
//! [`super::host_env_from_process`], which drops a `GOPROXY` carrying URL
//! userinfo and never reads `MAVEN_OPTS` (proxy passwords).

use super::{Consumer, DeveloperTools, StageCache};
use crate::sandbox::MergedSandboxConfig;
use shell_escape::escape;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// The [`Consumer::Session`] environment as shell-escaped `NAME=value \`
/// continuation lines, for the wrapper's `exec env -i` invocation. Assignments
/// after the allowlisted ones win, so the exported `PATH` (developer tools
/// first on macOS) replaces the allowlist's. A value that is not UTF-8 cannot
/// be written into the script and is left out. `cache` carries the session's
/// scratch directory ([`StageCache::with_socket_dir`]) for its sccache socket.
pub fn wrapper_exports(
    cache: &StageCache,
    worktree_root: &Path,
    host_env: &BTreeMap<String, OsString>,
) -> String {
    let developer = DeveloperTools::detect();
    cache
        .environment(
            Consumer::Session,
            worktree_root,
            host_env,
            developer.as_ref(),
        )
        .iter()
        .filter_map(|(name, value)| {
            let assignment = escape(format!("{name}={}", value.to_str()?).into());
            Some(format!("    {assignment} \\\n"))
        })
        .collect()
}

/// Let the session write the stage cache root, where its package-manager
/// caches live; the host package caches are not session-writable. The root
/// must exist when the session starts: Claude Code skips a missing
/// `allowWrite` path ([`StageCache::ensure`]).
pub fn apply_session_grant(cache: &StageCache, config: &mut MergedSandboxConfig) {
    let root = cache.root().display().to_string();
    if !config.filesystem.allow_write.contains(&root) {
        config.filesystem.allow_write.push(root);
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
