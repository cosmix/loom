//! The lines `loom status --web` prints at startup: the URL an operator should
//! open, and the remote-mode caveats.

use std::net::{IpAddr, SocketAddr};

use crate::web_server::interfaces;

/// Print [`startup_lines`] for a server bound to `local`, enumerating the
/// local interfaces when the bind is a wildcard.
pub(super) fn print_startup(local: SocketAddr, remote: bool, terminals: bool, token: Option<&str>) {
    let addresses = if remote && local.ip().is_unspecified() {
        interfaces::matching_addresses(local.ip())
    } else {
        Vec::new()
    };
    for line in startup_lines(local, remote, terminals, token, &addresses) {
        println!("{line}");
    }
}

/// The startup lines for a server bound to `local`. `addresses` are the
/// interface addresses advertised for a wildcard bind; any other bind ignores
/// them.
pub fn startup_lines(
    local: SocketAddr,
    remote: bool,
    terminals: bool,
    token: Option<&str>,
    addresses: &[IpAddr],
) -> Vec<String> {
    if !remote {
        let line = match token {
            Some(token) if terminals => format!(
                "loom dashboard: http://{local}/?token={token}  (terminals enabled; Ctrl-C to stop)"
            ),
            _ => format!("loom dashboard: http://{local}/  (Ctrl-C to stop)"),
        };
        return vec![line];
    }
    let token = token.unwrap_or_default();
    let note = if terminals { "; terminals enabled" } else { "" };
    let mut lines = vec![format!(
        "loom dashboard listening on {local} (remote access enabled{note})"
    )];
    if local.ip().is_unspecified() {
        // A wildcard bind's own address names no reachable interface, so
        // advertise every configured address in the same IP family.
        lines.extend(wildcard_bootstrap_lines(
            local.port(),
            token,
            addresses.iter().copied(),
        ));
    } else {
        // A concrete non-loopback bind's own address is directly reachable,
        // and the listener is not reachable on 127.0.0.1 - so the printed URL
        // is the real one rather than a placeholder plus a loopback fallback.
        lines.push(format!(
            "  bootstrap from another machine at: http://{local}/?token={token}"
        ));
    }
    lines.extend(remote_warning_lines(terminals));
    lines
}

pub(super) fn wildcard_bootstrap_lines(
    port: u16,
    token: &str,
    addresses: impl IntoIterator<Item = IpAddr>,
) -> Vec<String> {
    addresses
        .into_iter()
        .map(|ip| {
            let location = if ip.is_loopback() {
                "this machine"
            } else {
                "another machine"
            };
            let endpoint = SocketAddr::new(ip, port);
            format!("  bootstrap from {location} at: http://{endpoint}/?token={token}")
        })
        .collect()
}

/// Viewing a remote dashboard needs no token; the token grants the writes.
fn remote_warning_lines(terminals: bool) -> [String; 2] {
    let terminal = if terminals { " and terminal" } else { "" };
    [
        format!("  warning: plain HTTP; anyone who can reach this address can view the dashboard, and the token above grants settings{terminal} access to anyone who has it"),
        "  (Ctrl-C to stop)".to_owned(),
    ]
}
