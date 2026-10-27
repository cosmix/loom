//! A server's access policy: which requests reach which routes.
//!
//! Built once from the listener's bound address before any connection is
//! accepted ([`AccessPolicy::new`]), then threaded into every accepted
//! connection alongside that connection's own `local_addr` for `Host`
//! validation - a wildcard bind's accepted local endpoint varies per client,
//! even though the listener's own bind address never changes. Loopback mode
//! keeps the dashboard's original relaxed behaviour exactly; every other
//! bind - including a wildcard bind an individual client happens to reach
//! over loopback - runs the strict remote policy for the lifetime of the
//! server.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use super::auth::Auth;
use super::http::{self, RequestHead};

/// Whether an `Origin` header may be absent on an otherwise-permitted route.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OriginRequirement {
    /// Ordinary navigation and asset `GET`/`HEAD`: a same-origin browser
    /// request carries no `Origin` at all.
    Optional,
    /// A write, or a WebSocket upgrade: browsers attach `Origin` to both.
    Required,
}

/// Immutable for the life of one server: built once from the listener's
/// bound address and never re-derived per connection.
#[derive(Debug, Clone)]
pub struct AccessPolicy {
    remote: bool,
    auth: Option<Auth>,
}

impl AccessPolicy {
    /// The policy for a listener bound to `local_addr`. A non-loopback
    /// (including wildcard) bind is remote; [`crate::web_server::serve`]
    /// refuses to serve a remote policy without `auth`.
    pub fn new(local_addr: SocketAddr, auth: Option<Auth>) -> Self {
        Self {
            remote: !local_addr.ip().is_loopback(),
            auth,
        }
    }

    /// Whether the bind is not loopback, so the strict remote checks apply.
    pub fn is_remote(&self) -> bool {
        self.remote
    }

    /// The process token and cookie this server authenticates with, if any.
    pub fn auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    /// Whether this request carries the cookie a write route requires when
    /// the policy holds an [`Auth`]. Viewing routes never ask. Always true
    /// without one, as in the dashboard's default loopback posture, which
    /// authenticates nothing beyond `Host`/`Origin`.
    pub fn authenticated(&self, cookie: Option<&str>) -> bool {
        match &self.auth {
            None => true,
            Some(auth) => auth.cookie_matches(cookie),
        }
    }

    /// Validate `Host` against `local`, the socket address this particular
    /// connection was actually accepted on.
    pub fn host_allowed(&self, local: SocketAddr, head: &RequestHead) -> bool {
        if !self.remote {
            return http::host_allowed(head.host.as_deref());
        }
        match head.host.as_deref() {
            None => false,
            Some(host) => {
                parse_authority(host).is_some_and(|candidate| authority_matches(candidate, local))
            }
        }
    }

    /// Validate `Origin` against `local`. `requirement` states whether an
    /// absent header is acceptable on this route.
    pub fn origin_allowed(
        &self,
        local: SocketAddr,
        head: &RequestHead,
        requirement: OriginRequirement,
    ) -> bool {
        let require = requirement == OriginRequirement::Required;
        if !self.remote {
            return if require {
                http::origin_allowed_strict(head.origin.as_deref())
            } else {
                http::origin_allowed(head.origin.as_deref())
            };
        }
        match head.origin.as_deref() {
            None => !require,
            Some(origin) => remote_origin_authority(origin)
                .is_some_and(|candidate| authority_matches(candidate, local)),
        }
    }
}

/// A parsed remote authority: either a literal address or the exact
/// `localhost` alias, always paired with a port.
enum Authority {
    Literal(IpAddr),
    Localhost,
}

/// Parse a `Host`-shaped authority (`host:port` or `[v6]:port`), rejecting
/// userinfo, paths, DNS names other than the exact `localhost` alias, a
/// missing port, and an unbracketed literal that itself contains a colon
/// (the shape a bare, un-bracketed IPv6 literal takes).
fn parse_authority(raw: &str) -> Option<(Authority, u16)> {
    if raw.contains('@') || raw.contains('/') {
        return None;
    }
    if let Some(rest) = raw.strip_prefix('[') {
        let (host, remainder) = rest.split_once(']')?;
        let port_str = remainder.strip_prefix(':')?;
        let ip: Ipv6Addr = host.parse().ok()?;
        let port: u16 = port_str.parse().ok()?;
        return Some((Authority::Literal(IpAddr::V6(ip)), port));
    }
    let (host, port_str) = raw.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') || port_str.is_empty() {
        return None;
    }
    let port: u16 = port_str.parse().ok()?;
    if host.eq_ignore_ascii_case("localhost") {
        return Some((Authority::Localhost, port));
    }
    let ip: IpAddr = host.parse().ok()?;
    Some((Authority::Literal(ip), port))
}

/// [`parse_authority`] for an `Origin`'s `scheme://authority`, requiring an
/// exact `http` scheme and no path, query, fragment, or userinfo.
fn remote_origin_authority(origin: &str) -> Option<(Authority, u16)> {
    let (scheme, remainder) = origin.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    if remainder.contains(['/', '?', '#']) {
        return None;
    }
    parse_authority(remainder)
}

/// Whether a parsed authority names `local` exactly: same port, and either
/// the exact IP (after normalizing an IPv4-mapped IPv6 address on either
/// side) or `localhost` at a genuinely loopback endpoint.
fn authority_matches(candidate: (Authority, u16), local: SocketAddr) -> bool {
    let (authority, port) = candidate;
    if port != local.port() {
        return false;
    }
    let local_ip = normalize(local.ip());
    match authority {
        Authority::Localhost => local_ip.is_loopback(),
        Authority::Literal(ip) => normalize(ip) == local_ip,
    }
}

/// Collapse an IPv4-mapped IPv6 address to its IPv4 form, so a dual-stack
/// listener's reported endpoint compares equal to the plain IPv4 literal a
/// client's `Host`/`Origin` names.
fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        other => other,
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod access_tests;
