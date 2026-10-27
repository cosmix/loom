//! The planning server through `start` on real loopback sockets: binding, sharing, the gate
//! ([`socket`]) and WebSockets ([`ws`]). [`support`] holds the shared fixtures, which route
//! modules' own tests use too.

use std::net::{IpAddr, Ipv4Addr, TcpListener};

use serial_test::serial;

use super::{start, DEFAULT_PORT};
use support::{bootstrap, get, json_body, options, project, serve, serve_with, skip, status};

mod socket;
pub(crate) mod support;
mod ws;

#[test]
fn share_on_loopback_gives_the_tokenized_url_and_a_tunnel() {
    if skip("share_on_loopback_gives_the_tokenized_url_and_a_tunnel") {
        return;
    }
    let project = project();
    let server = serve(&project);
    let port = server.local.port();
    let cookies = bootstrap(&server);
    let share = get(&server, "/api/share", &cookies);
    assert_eq!(status(&share), 200, "{share}");
    let link = json_body(&share);
    let url = format!("http://127.0.0.1:{port}/?token={}", server.token);
    assert_eq!(link["url"], url.as_str());
    assert_eq!(link["bind"], "loopback");
    let tunnel = link["tunnel"].as_str().expect("a tunnel command");
    assert!(
        tunnel.starts_with(&format!("ssh -L {port}:127.0.0.1:{port} ")),
        "{tunnel}"
    );
    let info = json_body(&get(&server, "/api/server", &cookies));
    assert_eq!(info["remote"], false);
    assert_eq!(info["port"], port);
    server.stop().expect("stop");
}

#[test]
fn a_wildcard_bind_is_remote_and_offers_no_tunnel() {
    if skip("a_wildcard_bind_is_remote_and_offers_no_tunnel") {
        return;
    }
    let project = project();
    let mut opts = options(&project);
    opts.host = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let Ok(server) = super::start_with_routes(opts, super::routes::all_routes()) else {
        eprintln!("a_wildcard_bind_is_remote_and_offers_no_tunnel: wildcard bind unavailable");
        return;
    };
    assert!(server.ctx.bind().remote);
    let cookies = bootstrap(&server);
    let info = json_body(&get(&server, "/api/server", &cookies));
    assert_eq!(info["remote"], true, "{info}");
    let link = json_body(&get(&server, "/api/share", &cookies));
    assert_eq!(link["bind"], "remote");
    assert!(link["tunnel"].is_null(), "{link}");
    assert!(link["url"].as_str().unwrap_or("").contains(&server.token));
    server.stop().expect("stop");
}

#[test]
fn port_zero_binds_an_os_port() {
    if skip("port_zero_binds_an_os_port") {
        return;
    }
    let project = project();
    let server = serve(&project);
    assert_ne!(server.local.port(), 0);
    assert_eq!(server.ctx.bind().local, server.local);
    server.stop().expect("stop");
}

#[test]
fn an_explicit_busy_port_is_an_error_naming_the_address() {
    if skip("an_explicit_busy_port_is_an_error_naming_the_address") {
        return;
    }
    let project = project();
    let held = TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = held.local_addr().expect("held address").port();
    let mut opts = options(&project);
    opts.port = Some(port);
    let Err(error) = start(opts) else {
        panic!("a busy explicit port must fail without falling back");
    };
    assert!(
        format!("{error:#}").contains(&format!("127.0.0.1:{port}")),
        "{error:#}"
    );
}

#[test]
#[serial]
fn an_omitted_port_walks_past_a_busy_default() {
    if skip("an_omitted_port_walks_past_a_busy_default") {
        return;
    }
    let Ok(held) = TcpListener::bind(("127.0.0.1", DEFAULT_PORT)) else {
        eprintln!("an_omitted_port_walks_past_a_busy_default: {DEFAULT_PORT} is in use elsewhere");
        return;
    };
    let next_free = TcpListener::bind(("127.0.0.1", DEFAULT_PORT + 1)).is_ok();
    let project = project();
    let mut opts = options(&project);
    opts.port = None;
    let server = serve_with(opts);
    let port = server.local.port();
    if next_free {
        assert_eq!(port, DEFAULT_PORT + 1);
    } else {
        assert!(port > DEFAULT_PORT + 1, "{port}");
    }
    drop(held);
    server.stop().expect("stop");
}
