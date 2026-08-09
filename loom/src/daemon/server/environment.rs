//! Minimal environment inherited by the long-lived daemon process.

use std::ffi::{OsStr, OsString};

const HOST_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "TMUX",
    "TMUX_PANE",
    "TMUX_TMPDIR",
    "SSH_TTY",
    "TMPDIR",
    "TMP",
    "TEMP",
];

const LOOM_CONTROL_ALLOWLIST: &[&str] = &["LOOM_HOOKS_DIR", "LOOM_TERMINAL"];

/// Snapshot of the small host environment needed after daemonization.
pub(super) struct DaemonEnvironment {
    variables: Vec<(OsString, OsString)>,
}

impl DaemonEnvironment {
    pub(super) fn capture() -> Self {
        Self::capture_from(std::env::vars_os())
    }

    /// Clear inherited process state and restore only the captured allowlist.
    ///
    /// The caller invokes this in the single-threaded post-fork grandchild,
    /// before any daemon worker or stage process can observe ambient secrets.
    pub(super) fn apply(self) {
        let inherited_keys: Vec<OsString> = std::env::vars_os().map(|(key, _)| key).collect();
        for key in inherited_keys {
            std::env::remove_var(key);
        }
        for (key, value) in self.variables {
            std::env::set_var(key, value);
        }
    }

    fn capture_from<I, K, V>(source: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        let variables = source
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .filter(|(key, _)| is_allowed(key))
            .collect();
        Self { variables }
    }
}

fn is_allowed(key: &OsStr) -> bool {
    HOST_ENV_ALLOWLIST
        .iter()
        .chain(LOOM_CONTROL_ALLOWLIST)
        .any(|allowed| key == OsStr::new(allowed))
        || key.as_encoded_bytes().starts_with(b"LC_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_canaries_and_privileged_loom_values_are_not_captured() {
        let environment = DaemonEnvironment::capture_from([
            ("HOME", "/safe/home"),
            ("PATH", "/usr/bin:/bin"),
            ("LC_MESSAGES", "en_GB.UTF-8"),
            ("TERM", "xterm-256color"),
            ("LOOM_TERMINAL", "kitty"),
            ("LOOM_ADMIN_TOKEN", "secret-canary"),
            ("LOOM_ADMIN_PROOF", "secret-canary"),
            ("LOOM_STAGE_ID", "ambient-stage"),
            ("AWS_SECRET_ACCESS_KEY", "secret-canary"),
            ("GITHUB_TOKEN", "secret-canary"),
        ]);
        let keys: Vec<&OsStr> = environment
            .variables
            .iter()
            .map(|(key, _)| key.as_os_str())
            .collect();

        assert!(keys.contains(&OsStr::new("HOME")));
        assert!(keys.contains(&OsStr::new("PATH")));
        assert!(keys.contains(&OsStr::new("LC_MESSAGES")));
        assert!(keys.contains(&OsStr::new("TERM")));
        assert!(keys.contains(&OsStr::new("LOOM_TERMINAL")));
        assert!(!keys.contains(&OsStr::new("LOOM_ADMIN_TOKEN")));
        assert!(!keys.contains(&OsStr::new("LOOM_ADMIN_PROOF")));
        assert!(!keys.contains(&OsStr::new("LOOM_STAGE_ID")));
        assert!(!keys.contains(&OsStr::new("AWS_SECRET_ACCESS_KEY")));
        assert!(!keys.contains(&OsStr::new("GITHUB_TOKEN")));
    }
}
