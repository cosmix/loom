//! Minimal environment policy for processes that host stage agents.

use std::ffi::{OsStr, OsString};
use std::process::Command;

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
    // Rust toolchain locations. Both default to paths under HOME, so they are
    // usually absent — but installs that relocate them (CI images commonly set
    // CARGO_HOME=/usr/local/cargo) leave `cargo` unable to find its registry
    // and toolchains without them. Locations, not credentials.
    "CARGO_HOME",
    "RUSTUP_HOME",
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
}
