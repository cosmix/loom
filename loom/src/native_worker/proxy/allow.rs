//! CONNECT target allowlist: which `host:port` authorities the proxy may tunnel to.

use std::net::{Ipv4Addr, Ipv6Addr};

/// The only upstream port a tunnel may reach.
pub(super) const TUNNEL_PORT: &str = "443";
const MAX_HOST_BYTES: usize = 253;
const MAX_LABEL_BYTES: usize = 63;

/// `authority` is the CONNECT target, `host:port`. `allowed` entries are exact hosts or `*.suffix`.
///
/// The authority is split at the LAST colon, so `a:443@b:443` leaves `a:443@b` as the host and
/// the character check refuses it. The port must be exactly `443`. The host is lowercased (ASCII
/// only, so no Unicode case folding can turn a foreign character into an allowed one) and one
/// trailing dot is removed. IP literals and bracketed hosts are refused.
pub fn connect_target_allowed(authority: &str, allowed: &[&str]) -> bool {
    allowed_host(authority, allowed).is_some()
}

/// The normalized host of an allowed `authority`, or `None` when it is refused.
pub(super) fn allowed_host(authority: &str, allowed: &[&str]) -> Option<String> {
    let (host, port) = authority.rsplit_once(':')?;
    if port != TUNNEL_PORT {
        return None;
    }
    let host = normalize_host(host)?;
    allowed
        .iter()
        .any(|entry| entry_matches(&entry.to_ascii_lowercase(), &host))
        .then_some(host)
}

fn normalize_host(raw: &str) -> Option<String> {
    let lowered = raw.to_ascii_lowercase();
    let host = lowered.strip_suffix('.').unwrap_or(&lowered);
    if host.is_empty() || host.len() > MAX_HOST_BYTES || host.contains(['[', ']']) {
        return None;
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
    {
        return None;
    }
    if host
        .split('.')
        .any(|label| label.is_empty() || label.len() > MAX_LABEL_BYTES)
    {
        return None;
    }
    if host.parse::<Ipv4Addr>().is_ok() || host.parse::<Ipv6Addr>().is_ok() {
        return None;
    }
    Some(host.to_string())
}

fn entry_matches(entry: &str, host: &str) -> bool {
    match entry.strip_prefix("*.") {
        Some(suffix) => host
            .strip_suffix(suffix)
            .and_then(|rest| rest.strip_suffix('.'))
            .is_some_and(|label| !label.is_empty()),
        None => entry == host,
    }
}
