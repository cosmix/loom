//! Minimal environment policy for processes that host stage agents.

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Host values required for executable lookup, locale handling, and terminal
/// attachment. Authentication tokens and arbitrary ambient variables are not
/// inherited; Loom-specific values are supplied explicitly by the wrapper.
const STAGE_HOST_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "SHELL",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "TMUX_TMPDIR",
    "TMUX",
    "TMUX_PANE",
    "TMPDIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
];

/// Clear ambient process state and restore only the documented host allowlist.
pub fn apply_stage_environment(command: &mut Command) {
    apply_stage_environment_from(command, std::env::vars_os());
}

fn apply_stage_environment_from<I, K, V>(command: &mut Command, source: I)
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    command.env_clear();
    for (key, value) in source {
        let key = key.into();
        if is_allowed(&key) {
            command.env(key, value.into());
        }
    }
}

fn is_allowed(key: &OsStr) -> bool {
    STAGE_HOST_ENV_ALLOWLIST
        .iter()
        .any(|allowed| key == OsStr::new(allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_secret_canary_is_excluded_but_terminal_basics_survive() {
        let source = [
            ("HOME", "/safe/home"),
            ("PATH", "/usr/bin:/bin"),
            ("TERM", "xterm-256color"),
            ("HTTPS_PROXY", "http://proxy.example:8443"),
            ("GITHUB_TOKEN", "ambient-secret-canary"),
            ("AWS_SECRET_ACCESS_KEY", "ambient-secret-canary"),
        ];
        let mut command = Command::new("/usr/bin/env");
        apply_stage_environment_from(&mut command, source);

        let output = command.output().expect("the system env tool should run");
        let environment = String::from_utf8(output.stdout).unwrap();
        assert!(environment.contains("HOME=/safe/home"));
        assert!(environment.contains("TERM=xterm-256color"));
        assert!(environment.contains("HTTPS_PROXY=http://proxy.example:8443"));
        assert!(!environment.contains("ambient-secret-canary"));
        assert!(!environment.contains("GITHUB_TOKEN"));
    }
}
