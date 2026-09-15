//! Socket-level tests for the remote (non-loopback) access policy: a real
//! listener bound to `0.0.0.0:0`, reached over `127.0.0.1`, so a wildcard bind
//! is exercised exactly as [`AccessPolicy::resolve`] classifies it - remote,
//! even though this particular client happens to arrive over loopback.
//!
//! Skipped, with an explicit `SKIP` line, wherever the sandbox will not let a
//! test bind a wildcard socket at all.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

use super::{request, stop, workspace};
use crate::commands::status::web::{self, config_api, ServeOptions};
use crate::process::sandbox_probe::skip_unless;

/// A fixed 64-lowercase-hex process token, valid by construction.
fn token() -> String {
    "1234567890abcdef".repeat(4)
}

/// `ServeOptions` for a remote dashboard with terminals left disabled.
fn dashboard_options() -> ServeOptions {
    ServeOptions {
        dashboard_token: Some(token()),
        terminal_token: None,
    }
}

fn skip_without_wildcard(test_name: &str) -> bool {
    skip_unless(
        TcpListener::bind("0.0.0.0:0").is_ok(),
        test_name,
        "wildcard TCP bind is unavailable",
    )
}

/// Start a dashboard on a wildcard listener bound to an ephemeral port.
/// Panics rather than skips on bind failure: callers guard with
/// [`skip_without_wildcard`] first.
fn start_dashboard(base: PathBuf, options: ServeOptions) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("0.0.0.0:0").expect("bind wildcard listener");
    let port = listener.local_addr().expect("listener address").port();
    let running = Arc::new(AtomicBool::new(true));
    let serve_running = running.clone();
    thread::spawn(move || {
        let _ = web::serve(listener, base, serve_running, options);
    });
    (port, running)
}

/// A `GET /?token=` bootstrap request, with the option to attach `Origin`.
fn bootstrap_request(port: u16, presented_token: &str, origin: Option<&str>) -> String {
    let origin_header = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "GET /?token={presented_token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{origin_header}\r\n"
    )
}

/// The `Set-Cookie` header's full value, if the response carries one.
fn set_cookie_header(response: &str) -> Option<&str> {
    response
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
}

/// Bootstrap a valid cookie against `port`, returning the `name=value` pair
/// a subsequent request's `Cookie` header needs.
fn bootstrap_cookie(port: u16) -> String {
    let response = request(port, &bootstrap_request(port, &token(), None));
    assert!(response.starts_with("HTTP/1.1 302"), "{response}");
    let header = set_cookie_header(&response).expect("bootstrap response carries Set-Cookie");
    header.split(';').next().unwrap_or_default().to_owned()
}

#[test]
fn serve_fails_closed_without_a_valid_token() {
    if skip_without_wildcard("serve_fails_closed_without_a_valid_token") {
        return;
    }
    let (_temp, base) = workspace();
    let listener = TcpListener::bind("0.0.0.0:0").expect("bind wildcard listener");
    let running = Arc::new(AtomicBool::new(true));
    let result = web::serve(listener, base, running, ServeOptions::default());
    assert!(result.is_err(), "serve must fail closed without a token");
}

#[test]
fn serve_fails_closed_on_a_malformed_token() {
    if skip_without_wildcard("serve_fails_closed_on_a_malformed_token") {
        return;
    }
    let (_temp, base) = workspace();
    let listener = TcpListener::bind("0.0.0.0:0").expect("bind wildcard listener");
    let running = Arc::new(AtomicBool::new(true));
    let options = ServeOptions {
        dashboard_token: Some("not-a-valid-token".to_owned()),
        terminal_token: None,
    };
    let result = web::serve(listener, base, running, options);
    assert!(
        result.is_err(),
        "serve must fail closed on a malformed token"
    );
}

#[test]
fn unauthenticated_requests_are_refused_everywhere() {
    if skip_without_wildcard("unauthenticated_requests_are_refused_everywhere") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base, dashboard_options());
    let host = format!("Host: 127.0.0.1:{port}\r\n");
    let get = |path: &str| request(port, &format!("GET {path} HTTP/1.1\r\n{host}\r\n"));

    let responses = [
        get("/"),
        get("/assets/app.js"),
        get("/stages/anything"),
        get("/api/status"),
        get("/api/config"),
        request(
            port,
            &format!("POST /api/config HTTP/1.1\r\n{host}Content-Length: 2\r\n\r\n{{}}"),
        ),
        request(
            port,
            &format!(
                "GET /ws HTTP/1.1\r\n{host}Upgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
            ),
        ),
        get(&format!("/api/status?token={}", token())),
    ];
    stop(running);

    for response in &responses {
        assert!(response.starts_with("HTTP/1.1 401"), "{response}");
    }
}

#[test]
fn bootstrap_gates_on_token_and_origin() {
    if skip_without_wildcard("bootstrap_gates_on_token_and_origin") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base, dashboard_options());

    let valid = request(port, &bootstrap_request(port, &token(), None));
    let bad_token = request(port, &bootstrap_request(port, &"0".repeat(64), None));
    let foreign_origin = request(
        port,
        &bootstrap_request(port, &token(), Some("http://evil.example")),
    );
    stop(running);

    assert!(valid.starts_with("HTTP/1.1 302"), "{valid}");
    let cookie_header = set_cookie_header(&valid).expect("Set-Cookie present");
    assert!(cookie_header.contains("HttpOnly"), "{cookie_header}");
    assert!(cookie_header.contains("SameSite=Strict"), "{cookie_header}");

    assert!(bad_token.starts_with("HTTP/1.1 403"), "{bad_token}");
    assert!(
        foreign_origin.starts_with("HTTP/1.1 403"),
        "{foreign_origin}"
    );
}

#[test]
fn host_header_is_checked_even_with_a_valid_cookie() {
    if skip_without_wildcard("host_header_is_checked_even_with_a_valid_cookie") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base, dashboard_options());
    let cookie = bootstrap_cookie(port);

    let wrong_port = request(
        port,
        &format!(
            "GET /api/status HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {cookie}\r\n\r\n",
            port.wrapping_add(1)
        ),
    );
    let dns_name = request(
        port,
        &format!(
            "GET /api/status HTTP/1.1\r\nHost: evil.example:{port}\r\nCookie: {cookie}\r\n\r\n"
        ),
    );
    let wildcard_host = request(
        port,
        &format!("GET /api/status HTTP/1.1\r\nHost: 0.0.0.0:{port}\r\nCookie: {cookie}\r\n\r\n"),
    );
    stop(running);

    for response in [&wrong_port, &dns_name, &wildcard_host] {
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    }
}

/// A `POST /api/config` request carrying the given CSRF header value and an
/// `Origin` header, or no `Origin` at all when `origin` is `None`.
fn config_write_request(host: &str, cookie: &str, origin: Option<&str>, csrf: &str) -> String {
    let payload = r#"{"scope":"project","name":"terminal.backend","value":"tmux"}"#;
    let origin_header = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST /api/config HTTP/1.1\r\n{host}Cookie: {cookie}\r\n{origin_header}\
         Content-Type: application/json\r\nX-Loom-Csrf: {csrf}\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    )
}

#[test]
fn authenticated_reads_succeed() {
    if skip_without_wildcard("authenticated_reads_succeed") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base, dashboard_options());
    let cookie = bootstrap_cookie(port);
    let host = format!("Host: 127.0.0.1:{port}\r\n");

    let status = request(
        port,
        &format!("GET /api/status HTTP/1.1\r\n{host}Cookie: {cookie}\r\n\r\n"),
    );
    let config = request(
        port,
        &format!("GET /api/config HTTP/1.1\r\n{host}Cookie: {cookie}\r\n\r\n"),
    );
    stop(running);

    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert!(config.starts_with("HTTP/1.1 200"), "{config}");
}

#[test]
fn a_refused_write_never_touches_the_config_file() {
    if skip_without_wildcard("a_refused_write_never_touches_the_config_file") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base.clone(), dashboard_options());
    let cookie = bootstrap_cookie(port);
    let host = format!("Host: 127.0.0.1:{port}\r\n");

    let config_path = base.join(".loom/work/config.toml");
    let before = std::fs::read(&config_path).unwrap_or_default();

    let csrf = config_api::test_token();
    let no_origin = request(port, &config_write_request(&host, &cookie, None, csrf));
    let bad_csrf = request(
        port,
        &config_write_request(
            &host,
            &cookie,
            Some(&format!("http://127.0.0.1:{port}")),
            &"0".repeat(64),
        ),
    );
    let after = std::fs::read(&config_path).unwrap_or_default();
    stop(running);

    assert!(no_origin.starts_with("HTTP/1.1 403"), "{no_origin}");
    assert!(bad_csrf.starts_with("HTTP/1.1 403"), "{bad_csrf}");
    assert_eq!(
        before, after,
        "a refused write must not touch the config file"
    );
}

#[test]
fn a_dashboard_cookie_alone_does_not_enable_terminals() {
    if skip_without_wildcard("a_dashboard_cookie_alone_does_not_enable_terminals") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start_dashboard(base, dashboard_options());
    let cookie = bootstrap_cookie(port);

    let response = request(
        port,
        &format!(
            "GET /ws/terminal/testfake/view HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\
             Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
        ),
    );
    stop(running);

    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}

#[test]
fn distinct_ports_get_distinct_cookie_names() {
    if skip_without_wildcard("distinct_ports_get_distinct_cookie_names") {
        return;
    }
    let (_temp_a, base_a) = workspace();
    let (_temp_b, base_b) = workspace();
    let (port_a, running_a) = start_dashboard(base_a, dashboard_options());
    let (port_b, running_b) = start_dashboard(base_b, dashboard_options());

    let cookie_a = bootstrap_cookie(port_a);
    let cookie_b = bootstrap_cookie(port_b);
    stop(running_a);
    stop(running_b);

    let name_a = cookie_a.split('=').next().unwrap_or_default();
    let name_b = cookie_b.split('=').next().unwrap_or_default();
    assert_ne!(name_a, name_b);
    assert_eq!(name_a, web::cookie_name_for_port(port_a));
    assert_eq!(name_b, web::cookie_name_for_port(port_b));
}
