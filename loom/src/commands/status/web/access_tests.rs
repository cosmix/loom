//! Pure unit tests for [`AccessPolicy`]: constructed socket addresses and
//! request heads only, no listener, network, process, or clock dependency.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::{AccessPolicy, OriginRequirement};
use crate::commands::status::web::http::RequestHead;
use crate::commands::status::web::ServeOptions;

const REMOTE_TOKEN: &str = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";

fn head(host: Option<&str>, origin: Option<&str>) -> RequestHead {
    RequestHead {
        method: "GET".to_owned(),
        path: "/".to_owned(),
        query: None,
        upgrade_websocket: false,
        origin: origin.map(str::to_owned),
        host: host.map(str::to_owned),
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    }
}

fn loopback_policy() -> AccessPolicy {
    AccessPolicy::resolve(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373),
        &ServeOptions::default(),
    )
    .expect("loopback policy resolves")
}

fn remote_policy() -> AccessPolicy {
    AccessPolicy::resolve(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7373),
        &ServeOptions {
            dashboard_token: Some(REMOTE_TOKEN.to_owned()),
            ..Default::default()
        },
    )
    .expect("remote policy resolves with a valid token")
}

#[test]
fn a_remote_bind_without_a_token_fails_to_resolve() {
    let error = AccessPolicy::resolve(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7373),
        &ServeOptions::default(),
    )
    .expect_err("a non-loopback bind must fail closed without a token");
    assert!(error.to_string().contains("non-loopback"));
}

#[test]
fn default_loopback_compatibility_is_unchanged() {
    let policy = loopback_policy();
    assert!(!policy.is_remote());
    for host in ["127.0.0.1", "localhost", "127.0.0.1:41599"] {
        assert!(policy.host_allowed(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373),
            &head(Some(host), None)
        ));
    }
    assert!(!policy.host_allowed(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373),
        &head(Some("evil.example"), None)
    ));
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373);
    assert!(policy.origin_allowed(local, &head(None, None), OriginRequirement::Optional));
    assert!(policy.origin_allowed(
        local,
        &head(None, Some("http://127.0.0.1:7373")),
        OriginRequirement::Optional
    ));
    assert!(!policy.origin_allowed(local, &head(None, None), OriginRequirement::Required));
    assert!(policy.authenticated(None), "loopback needs no cookie");
}

#[test]
fn remote_host_must_match_the_accepted_local_endpoint_exactly() {
    let policy = remote_policy();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 7373);
    assert!(policy.host_allowed(local, &head(Some("203.0.113.5:7373"), None)));
    assert!(!policy.host_allowed(local, &head(Some("203.0.113.5:9999"), None)));
    assert!(!policy.host_allowed(local, &head(Some("198.51.100.9:7373"), None)));
    assert!(!policy.host_allowed(local, &head(None, None)));
    assert!(!policy.host_allowed(local, &head(Some("evil.example:7373"), None)));
}

#[test]
fn remote_wildcard_listener_still_requires_the_accepted_endpoint() {
    let policy = remote_policy();
    // Even though the listener itself is bound to the wildcard address, a
    // client that happens to arrive over loopback is validated against its
    // own accepted local endpoint, not the listener's bind address.
    let loopback_local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373);
    assert!(policy.host_allowed(loopback_local, &head(Some("127.0.0.1:7373"), None)));
    assert!(policy.host_allowed(loopback_local, &head(Some("localhost:7373"), None)));
    // The listener's own wildcard address is never a valid destination.
    assert!(!policy.host_allowed(loopback_local, &head(Some("0.0.0.0:7373"), None)));
}

#[test]
fn remote_localhost_alias_requires_a_genuinely_loopback_endpoint() {
    let policy = remote_policy();
    let non_loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 7373);
    assert!(!policy.host_allowed(non_loopback, &head(Some("localhost:7373"), None)));
}

#[test]
fn remote_host_rejects_malformed_and_hostile_authorities() {
    let policy = remote_policy();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 7373);
    for host in [
        "203.0.113.5",
        "user@203.0.113.5:7373",
        "203.0.113.5:7373/path",
        "[203.0.113.5:7373",
        "::1:7373",
        "203.0.113.5:",
        "203.0.113.5:abc",
    ] {
        assert!(
            !policy.host_allowed(local, &head(Some(host), None)),
            "{host}"
        );
    }
}

#[test]
fn remote_ipv6_authority_matches_bracketed_form() {
    let policy = remote_policy();
    let local = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 7373);
    assert!(policy.host_allowed(local, &head(Some("[::1]:7373"), None)));
    assert!(!policy.host_allowed(local, &head(Some("[::1]:9999"), None)));
    assert!(!policy.host_allowed(local, &head(Some("[::2]:7373"), None)));
}

#[test]
fn remote_ipv4_mapped_ipv6_normalizes_to_ipv4() {
    let policy = remote_policy();
    let mapped = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xcb00, 0x7105); // ::ffff:203.0.113.5
    let local = SocketAddr::new(IpAddr::V6(mapped), 7373);
    assert!(policy.host_allowed(local, &head(Some("203.0.113.5:7373"), None)));
}

#[test]
fn remote_origin_accepts_the_matching_authority_and_refuses_everything_else() {
    let policy = remote_policy();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 7373);
    assert!(policy.origin_allowed(
        local,
        &head(None, Some("http://203.0.113.5:7373")),
        OriginRequirement::Required
    ));
    for origin in [
        "https://203.0.113.5:7373",
        "http://203.0.113.5:9999",
        "http://198.51.100.9:7373",
        "http://203.0.113.5:7373/path",
        "http://203.0.113.5:7373?x=1",
        "http://203.0.113.5:7373#frag",
        "null",
        "http://user@203.0.113.5:7373",
    ] {
        assert!(
            !policy.origin_allowed(
                local,
                &head(None, Some(origin)),
                OriginRequirement::Required
            ),
            "{origin}"
        );
    }
}

#[test]
fn remote_origin_presence_follows_the_requirement() {
    let policy = remote_policy();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 7373);
    assert!(policy.origin_allowed(local, &head(None, None), OriginRequirement::Optional));
    assert!(!policy.origin_allowed(local, &head(None, None), OriginRequirement::Required));
}

#[test]
fn remote_dashboard_auth_gates_every_route_by_cookie() {
    let policy = remote_policy();
    assert!(!policy.authenticated(None));
    assert!(!policy.authenticated(Some("loom_dashboard_7373=wrong")));
    assert!(policy.authenticated(Some(&format!("loom_dashboard_7373={REMOTE_TOKEN}"))));
}
