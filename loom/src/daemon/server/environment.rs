//! Minimal environment inherited by the long-lived daemon process.

use std::ffi::{OsStr, OsString};
use std::process::Command;

use crate::git::signing::SIGNING_ENV_NAMES;

const HOST_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "TERM",
    // TERMINFO/TERMINFO_DIRS locations, paired with TERM above — together
    // with HOME (already forwarded, which covers `~/.terminfo`) these are the
    // standard ncurses resolution inputs. TERM only names the terminal;
    // these say where its capability database lives, and forwarding the name
    // without the database forwards half a contract: any terminal whose
    // terminfo entry is not bundled into the system database (kitty is the
    // observed instance) leaves TERM unresolvable for anything spawned under
    // this environment, which is how a terminal-database problem can read as
    // "the server is not accepting clients".
    "TERMINFO",
    "TERMINFO_DIRS",
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
    // The quota poller is the daemon's first outbound HTTP caller (it polls
    // the claude.ai usage endpoint); without these a proxied network cannot
    // be reached from inside the daemon's stripped environment.
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    // Read by the daemon itself: `RUST_LOG` sets its tracing filter,
    // `LOOM_SCCACHE`, `RUSTC_WRAPPER` and the two `SCCACHE_*` names feed the
    // stage build cache (`orchestrator/terminal/native/build_cache.rs`), and
    // `LOOM_HOME` locates the user config (`user_config`).
    "RUST_LOG",
    "SCCACHE_DIR",
    "SCCACHE_CACHE_SIZE",
    "LOOM_SCCACHE",
    "RUSTC_WRAPPER",
    "LOOM_HOME",
    // Where git reads the user's configuration. Locations, not credentials:
    // without them the daemon could read another global config than the
    // operator, and so another `commit.gpgsign`.
    "XDG_CONFIG_HOME",
    "GIT_CONFIG_GLOBAL",
];

const LOOM_CONTROL_ALLOWLIST: &[&str] = &["LOOM_HOOKS_DIR", "LOOM_TERMINAL"];

/// Snapshot of the small host environment the daemon child is started with.
pub(super) struct DaemonEnvironment {
    variables: Vec<(OsString, OsString)>,
}

impl DaemonEnvironment {
    pub(super) fn capture() -> Self {
        Self::capture_from(std::env::vars_os())
    }

    /// Start `command` from an empty environment holding only the captured
    /// allowlist, so neither the daemon nor any stage process it starts can
    /// observe the ambient secrets of the shell that ran `loom run`.
    pub(super) fn apply_to(&self, command: &mut Command) {
        command.env_clear().envs(
            self.variables
                .iter()
                .map(|(key, value)| (key.as_os_str(), value.as_os_str())),
        );
    }

    pub(super) fn capture_from<I, K, V>(source: I) -> Self
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

/// The allowlisted subset of this process's environment: exactly what the
/// daemon child is started with.
pub(crate) fn daemon_environment_pairs() -> Vec<(OsString, OsString)> {
    DaemonEnvironment::capture().variables
}

/// A variable the daemon child receives. The signing variables reach the
/// daemon child only: it removes them from its own environment at startup
/// (`crate::git::signing::take_from_process`), so no stage process sees them.
fn is_allowed(key: &OsStr) -> bool {
    HOST_ENV_ALLOWLIST
        .iter()
        .chain(LOOM_CONTROL_ALLOWLIST)
        .chain(&SIGNING_ENV_NAMES)
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
            ("TERMINFO", "/home/user/.local/kitty.app/lib/kitty/terminfo"),
            ("TERMINFO_DIRS", "/usr/share/terminfo:/etc/terminfo"),
            ("LOOM_TERMINAL", "kitty"),
            ("HTTP_PROXY", "http://proxy.internal:8080"),
            ("HTTPS_PROXY", "http://proxy.internal:8080"),
            ("NO_PROXY", "localhost,127.0.0.1"),
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
        // See the HOST_ENV_ALLOWLIST comment above: TERM without its
        // terminfo location is half a contract.
        assert!(keys.contains(&OsStr::new("TERMINFO")));
        assert!(keys.contains(&OsStr::new("TERMINFO_DIRS")));
        assert!(keys.contains(&OsStr::new("LOOM_TERMINAL")));
        // The quota poller's outbound HTTP requests need these to honor a
        // proxied network from inside the daemon's stripped environment.
        assert!(keys.contains(&OsStr::new("HTTP_PROXY")));
        assert!(keys.contains(&OsStr::new("HTTPS_PROXY")));
        assert!(keys.contains(&OsStr::new("NO_PROXY")));
        assert!(!keys.contains(&OsStr::new("LOOM_ADMIN_TOKEN")));
        assert!(!keys.contains(&OsStr::new("LOOM_ADMIN_PROOF")));
        assert!(!keys.contains(&OsStr::new("LOOM_STAGE_ID")));
        assert!(!keys.contains(&OsStr::new("AWS_SECRET_ACCESS_KEY")));
        assert!(!keys.contains(&OsStr::new("GITHUB_TOKEN")));
    }

    #[test]
    fn rust_log_and_sccache_variables_are_captured() {
        let environment = DaemonEnvironment::capture_from([
            ("RUST_LOG", "loom=debug"),
            ("SCCACHE_DIR", "/safe/home/.cache/sccache"),
            ("SCCACHE_CACHE_SIZE", "10G"),
            ("LOOM_SCCACHE", "/opt/homebrew/bin/sccache"),
            ("RUSTC_WRAPPER", "/opt/homebrew/bin/sccache"),
            ("LOOM_HOME", "/safe/home/.loom"),
            ("LOOM_UNLISTED_SETTING", "ambient"),
        ]);
        let keys: Vec<&OsStr> = environment
            .variables
            .iter()
            .map(|(key, _)| key.as_os_str())
            .collect();

        for listed in [
            "RUST_LOG",
            "SCCACHE_DIR",
            "SCCACHE_CACHE_SIZE",
            "LOOM_SCCACHE",
            "RUSTC_WRAPPER",
            "LOOM_HOME",
        ] {
            assert!(keys.contains(&OsStr::new(listed)), "{listed} is captured");
        }
        assert!(!keys.contains(&OsStr::new("LOOM_UNLISTED_SETTING")));
    }

    #[test]
    fn signing_variables_reach_the_daemon_child_but_never_a_stage_environment() {
        let source = [
            ("HOME", "/safe/home"),
            ("GNUPGHOME", "/safe/home/.gnupg"),
            ("SSH_AUTH_SOCK", "/run/user/1000/ssh-agent.sock"),
            ("XDG_CONFIG_HOME", "/safe/home/.config"),
            ("GIT_CONFIG_GLOBAL", "/safe/home/.gitconfig"),
        ];
        let environment = DaemonEnvironment::capture_from(source);
        let keys: Vec<&OsStr> = environment
            .variables
            .iter()
            .map(|(key, _)| key.as_os_str())
            .collect();
        for (name, _) in source {
            assert!(
                keys.contains(&OsStr::new(name)),
                "{name} reaches the daemon"
            );
        }
        for (name, _) in daemon_environment_pairs() {
            assert!(is_allowed(&name), "{name:?} is outside the allowlist");
        }

        let stage_source = &source[..3];
        let mut command = Command::new("true");
        crate::process::apply_stage_environment_from(&mut command, stage_source.iter().copied());
        let stage: Vec<&OsStr> = command.get_envs().map(|(key, _)| key).collect();
        assert_eq!(stage, vec![OsStr::new("HOME")]);

        let session = crate::process::agent_session_environment_from(stage_source.iter().copied());
        for (name, _) in &session {
            assert!(
                !SIGNING_ENV_NAMES
                    .iter()
                    .any(|signing| name.as_os_str() == OsStr::new(signing)),
                "{name:?} reaches a stage session"
            );
        }
    }

    #[test]
    fn every_agent_session_variable_survives_the_daemon_capture() {
        for name in crate::process::AGENT_SESSION_ENV_NAMES {
            assert!(
                is_allowed(OsStr::new(name)),
                "{name} reaches stage sessions but the daemon drops it"
            );
        }
    }
}
