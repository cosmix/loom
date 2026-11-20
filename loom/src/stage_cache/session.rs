//! The stage cache as the stage's Claude session and its native workers see
//! it: the exports the session wrapper adds to its `exec env -i` line, the
//! sandbox write grant for the root, and the host environment both read their
//! seeds and developer tools from.

use super::{Consumer, DeveloperTools, StageCache};
use crate::sandbox::MergedSandboxConfig;
use shell_escape::escape;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// The host variables [`StageCache::environment`], [`super::seed_read_roots`]
/// and [`super::toolchain_read_roots`] read. Everything else in the daemon's
/// environment stays out of the map. The map still carries `GOPROXY` and
/// `MAVEN_OPTS`, which can hold a proxy URL's userinfo or a proxy password;
/// they reach the session as the operator wrote them.
const HOST_ENV_NAMES: [&str; 10] = [
    "HOME",
    "PATH",
    "GOPATH",
    "GOMODCACHE",
    "CCACHE_DIR",
    "XDG_CACHE_HOME",
    "GRADLE_USER_HOME",
    "GOPROXY",
    "GOFLAGS",
    "MAVEN_OPTS",
];

/// The daemon's own values for `HOST_ENV_NAMES`.
pub fn host_env_from_process() -> BTreeMap<String, OsString> {
    HOST_ENV_NAMES
        .iter()
        .filter_map(|name| Some((name.to_string(), std::env::var_os(name)?)))
        .collect()
}

/// The [`Consumer::Session`] environment as shell-escaped `NAME=value \`
/// continuation lines, for the wrapper's `exec env -i` invocation. Assignments
/// after the allowlisted ones win, so the exported `PATH` (developer tools
/// first on macOS) replaces the allowlist's. A value that is not UTF-8 cannot
/// be written into the script and is left out.
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

/// Let the session write the stage cache root, which replaces the host
/// package caches it was granted before. The root must exist when the session
/// starts: Claude Code skips a missing `allowWrite` path ([`StageCache::ensure`]).
pub fn apply_session_grant(cache: &StageCache, config: &mut MergedSandboxConfig) {
    let root = cache.root().display().to_string();
    if !config.filesystem.allow_write.contains(&root) {
        config.filesystem.allow_write.push(root);
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
