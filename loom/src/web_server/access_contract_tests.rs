//! Contracts of stage runtime-and-dashboard-server: on a loopback bind the
//! `Origin` must equal the request's own `Host`, and `Host` must be exactly a
//! loopback name with an optional port.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use super::access::{AccessPolicy, OriginRequirement};
use super::http::RequestHead;
use crate::commands::status::web::ServeOptions;

fn local() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7373)
}

fn loopback_policy() -> AccessPolicy {
    AccessPolicy::resolve(local(), &ServeOptions::default()).expect("loopback policy resolves")
}

fn head(host: &str, origin: Option<&str>) -> RequestHead {
    RequestHead {
        method: "GET".to_owned(),
        path: "/ws".to_owned(),
        query: None,
        upgrade_websocket: true,
        origin: origin.map(str::to_owned),
        host: Some(host.to_owned()),
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    }
}

#[test]
fn a_loopback_policy_refuses_an_origin_on_another_port() {
    let policy = loopback_policy();
    let allowed = |host: &str, origin: &str| {
        policy.origin_allowed(
            local(),
            &head(host, Some(origin)),
            OriginRequirement::Required,
        )
    };
    let refused = [
        ("127.0.0.1:7373", "http://localhost:3000"),
        ("127.0.0.1:7373", "http://127.0.0.1:5173"),
        ("localhost:8080", "http://localhost:3000"),
    ];
    for (host, origin) in refused {
        assert!(
            !allowed(host, origin),
            "Host {host} must refuse Origin {origin}"
        );
    }
    let accepted = [
        ("127.0.0.1:7373", "http://127.0.0.1:7373"),
        ("localhost:7373", "http://localhost:7373"),
        ("localhost:8080", "http://localhost:8080"),
    ];
    for (host, origin) in accepted {
        assert!(
            allowed(host, origin),
            "Host {host} must accept Origin {origin}"
        );
    }
}

#[test]
fn a_bracketed_ipv6_host_with_a_suffix_is_not_loopback() {
    let policy = loopback_policy();
    let allowed = |host: &str| policy.host_allowed(local(), &head(host, None));
    for host in [
        "[::1].evil.com:7373",
        "[::1].evil.com",
        "[::1",
        "127.0.0.1:80@evil.com",
    ] {
        assert!(!allowed(host), "Host {host} must be refused");
    }
    for host in ["127.0.0.1", "localhost", "[::1]", "[::1]:7373"] {
        assert!(allowed(host), "Host {host} must be accepted");
    }
}
