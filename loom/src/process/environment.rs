//! Minimal environment policy for processes that host stage agents.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Names the stage wrapper's `env -i` list forwards from the host environment
/// into an agent session. The wrapper's shell loop is generated from this list,
/// so the two cannot drift. `HOME` and `PATH` are handled separately (the
/// wrapper always writes both, with a `PATH` fallback). Locations and login
/// identity only, never credentials.
pub const AGENT_SESSION_ENV_NAMES: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TERMINFO",
    "TERMINFO_DIRS",
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
    "SCCACHE_DIR",
    "SCCACHE_CACHE_SIZE",
    // USER and LOGNAME name the operator and are not credentials. On macOS the
    // `claude` CLI finds its Keychain login by `$USER`.
    "USER",
    "LOGNAME",
];

/// Host values required for executable lookup, locale handling, and terminal
/// attachment. Authentication tokens and arbitrary ambient variables are not
/// inherited; Loom-specific values are supplied explicitly by the wrapper.
///
/// This list also governs plan-authored commands (see
/// [`crate::verify::criteria::spawn_confined`]), so it must carry enough for a
/// build toolchain to find itself — an acceptance criterion that cannot run
/// `cargo` fails the stage just as loudly as a real defect.
const STAGE_HOST_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "PATH",
    // USER and LOGNAME name the operator and are not credentials. On macOS the
    // `claude` CLI finds its Keychain login by `$USER`; without it every
    // session started under this environment reads "Not logged in".
    "USER",
    "LOGNAME",
    // Rust toolchain locations. Both default to paths under HOME, so they are
    // usually absent — but installs that relocate them (CI images commonly set
    // CARGO_HOME=/usr/local/cargo) leave `cargo` unable to find its registry
    // and toolchains without them. Locations, not credentials.
    "CARGO_HOME",
    "RUSTUP_HOME",
    // Cache settings are inert without a selected wrapper. An ambient wrapper
    // that fails in the stage sandbox (for example, sccache with EPERM) must
    // never sit between a confined criterion and rustc; launch-owned wrapper
    // policy lives in orchestrator/terminal/native/build_cache.rs.
    "SCCACHE_DIR",
    "SCCACHE_CACHE_SIZE",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    // TERMINFO/TERMINFO_DIRS locations, paired with TERM above — together
    // with HOME (already forwarded, which covers `~/.terminfo`) these are the
    // standard ncurses resolution inputs. TERM only names the terminal;
    // these say where its capability database lives, and forwarding the name
    // without the database forwards half a contract: any terminal whose
    // terminfo entry is not bundled into the system database (kitty is the
    // observed instance) leaves TERM unresolvable. A tmux control probe in
    // orchestrator/terminal/tmux/ built on an unresolvable TERM exits
    // non-zero, which reads identically to "the server is not accepting
    // clients".
    "TERMINFO",
    "TERMINFO_DIRS",
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
    // CA bundle LOCATIONS, not credentials — pair with the proxy variables
    // above. A host behind a corporate MITM proxy (the case HTTPS_PROXY
    // exists to serve) typically also needs a custom CA bundle for the TLS
    // handshake to succeed; forwarding one without the other leaves `cargo`
    // and other TLS clients unable to complete a fetch through that proxy.
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NIX_SSL_CERT_FILE",
    // SSH_AUTH_SOCK is deliberately WITHHELD: it is a live credential-agent
    // socket, not a location. An acceptance criterion needing SSH auth
    // (`git fetch` over SSH, a git-SSH cargo dependency) fails by design
    // rather than silently inheriting host agent access.
];

/// Clear ambient process state and restore only the documented host allowlist.
pub fn apply_stage_environment(command: &mut Command) {
    apply_stage_environment_from(command, std::env::vars_os());
}

/// [`apply_stage_environment`] over an explicit environment source.
pub fn apply_stage_environment_from<I, K, V>(command: &mut Command, source: I)
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

/// The environment a stage session's `claude` process runs under, mirroring
/// the wrapper's `env -i` list: `HOME` (empty when unset), `PATH` (falling back
/// to `/usr/bin:/bin` when unset or empty), then each
/// [`AGENT_SESSION_ENV_NAMES`] entry that has a non-empty value in `source`.
/// Everything else is dropped.
pub fn agent_session_environment_from<I, K, V>(source: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    let source: HashMap<OsString, OsString> = source
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    let non_empty = |name: &str| {
        source
            .get(OsStr::new(name))
            .filter(|value| !value.is_empty())
    };

    let home = source.get(OsStr::new("HOME")).cloned().unwrap_or_default();
    let path = non_empty("PATH")
        .cloned()
        .unwrap_or_else(|| OsString::from("/usr/bin:/bin"));
    let mut environment: Vec<(OsString, OsString)> =
        vec![("HOME".into(), home), ("PATH".into(), path)];
    environment.extend(
        AGENT_SESSION_ENV_NAMES
            .iter()
            .filter_map(|name| non_empty(name).map(|value| ((*name).into(), value.clone()))),
    );
    environment
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
            ("TERMINFO", "/home/user/.local/kitty.app/lib/kitty/terminfo"),
            ("TERMINFO_DIRS", "/usr/share/terminfo:/etc/terminfo"),
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
        // See the TERMINFO comment on STAGE_HOST_ENV_ALLOWLIST above: TERM
        // without its terminfo location is half a contract.
        assert!(environment.contains("TERMINFO=/home/user/.local/kitty.app/lib/kitty/terminfo"));
        assert!(environment.contains("TERMINFO_DIRS=/usr/share/terminfo:/etc/terminfo"));
        assert!(environment.contains("HTTPS_PROXY=http://proxy.example:8443"));
        assert!(!environment.contains("ambient-secret-canary"));
        assert!(!environment.contains("GITHUB_TOKEN"));
    }

    #[test]
    fn sccache_locations_survive_but_wrapper_is_excluded_from_a_confined_run() {
        let source = [
            ("HOME", "/safe/home"),
            ("PATH", "/usr/bin:/bin"),
            ("RUSTC_WRAPPER", "/opt/homebrew/bin/sccache"),
            ("SCCACHE_DIR", "/safe/home/.cache/sccache"),
            ("SCCACHE_CACHE_SIZE", "10G"),
        ];
        let mut command = Command::new("/usr/bin/env");
        apply_stage_environment_from(&mut command, source);

        let output = command.output().expect("the system env tool should run");
        let environment = String::from_utf8(output.stdout).unwrap();
        assert!(!environment.contains("RUSTC_WRAPPER"));
        assert!(environment.contains("SCCACHE_DIR=/safe/home/.cache/sccache"));
        assert!(environment.contains("SCCACHE_CACHE_SIZE=10G"));
    }

    #[test]
    fn agent_session_environment_keeps_user_logname_home_and_path() {
        let source = [
            ("HOME", "/home/alice"),
            ("PATH", "/opt/bin:/usr/bin"),
            ("USER", "alice"),
            ("LOGNAME", "alice"),
            ("LANG", "C.UTF-8"),
            ("GITHUB_TOKEN", "canary"),
        ];
        let environment = agent_session_environment_from(source);
        let pair = |k: &str, v: &str| (OsString::from(k), OsString::from(v));

        assert_eq!(environment[0], pair("HOME", "/home/alice"));
        assert_eq!(environment[1], pair("PATH", "/opt/bin:/usr/bin"));
        assert!(environment.contains(&pair("USER", "alice")));
        assert!(environment.contains(&pair("LOGNAME", "alice")));
        assert!(environment.contains(&pair("LANG", "C.UTF-8")));
        assert!(environment.iter().all(|(key, _)| key != "GITHUB_TOKEN"));
    }

    #[test]
    fn agent_session_environment_defaults_path_and_skips_empty_values() {
        let source = [("PATH", ""), ("USER", ""), ("LOGNAME", "alice")];
        let environment = agent_session_environment_from(source);
        let pair = |k: &str, v: &str| (OsString::from(k), OsString::from(v));

        assert_eq!(environment[0], pair("HOME", ""));
        assert_eq!(environment[1], pair("PATH", "/usr/bin:/bin"));
        assert!(environment.iter().all(|(key, _)| key != "USER"));
        assert!(environment.contains(&pair("LOGNAME", "alice")));
    }

    #[test]
    fn every_agent_session_name_is_in_the_stage_host_allowlist() {
        // A name the wrapper forwards that the host layer drops never reaches
        // the wrapper: the gap the first fix for issue #19 left.
        for name in AGENT_SESSION_ENV_NAMES {
            assert!(
                STAGE_HOST_ENV_ALLOWLIST.contains(name),
                "{name} is forwarded to agent sessions but missing from STAGE_HOST_ENV_ALLOWLIST"
            );
        }
    }

    #[test]
    fn stage_host_layer_keeps_user_and_logname() {
        let source = [
            ("HOME", "/safe/home"),
            ("PATH", "/usr/bin:/bin"),
            ("USER", "alice"),
            ("LOGNAME", "alice"),
            ("GITHUB_TOKEN", "ambient-secret-canary"),
        ];
        let mut command = Command::new("/usr/bin/env");
        apply_stage_environment_from(&mut command, source);

        let output = command.output().expect("the system env tool should run");
        let environment = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<&str> = environment.lines().collect();
        assert!(lines.contains(&"USER=alice"));
        assert!(lines.contains(&"LOGNAME=alice"));
        assert!(!environment.contains("ambient-secret-canary"));
    }
}
