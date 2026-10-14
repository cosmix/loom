//! The history route: what it answers for a session id, and the terminal
//! lane's gate in front of it.

use super::*;
use crate::commands::status::web::terminal::tests_upgrade::terminal_options;
use crate::commands::status::web::tests::{
    body, request, skip_without_loopback, start, start_with, stop, workspace,
};
use crate::orchestrator::terminal::tmux::{write_history, MAX_HISTORY_BYTES};

fn history_request(port: u16, session: &str, cookie: Option<&str>, origin: Option<&str>) -> String {
    let cookie = cookie.map(|cookie| format!("Cookie: {cookie}\r\n"));
    let origin = origin.map(|origin| format!("Origin: {origin}\r\n"));
    format!(
        "GET {ROUTE_PREFIX}{session} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{}{}\r\n",
        origin.unwrap_or_default(),
        cookie.unwrap_or_default()
    )
}

fn token_cookie(port: u16) -> String {
    format!(
        "{}={}",
        crate::commands::status::web::cookie_name_for_port(port),
        "a".repeat(64)
    )
}

fn work_root(base: &Path) -> std::path::PathBuf {
    WorkDir::new(base).unwrap().root().to_path_buf()
}

#[test]
fn a_saved_history_is_answered_with_its_bytes() {
    let (_temp, base) = workspace();
    let screen = b"\x1b[32mpassed\x1b[0m\nDone\n";
    write_history(&work_root(&base), "session-a", screen).unwrap();

    let (status, _, content_type, bytes) =
        history_reply(&base, &format!("{ROUTE_PREFIX}session-a"));

    assert_eq!((status, content_type), (200, "application/octet-stream"));
    assert_eq!(bytes, screen);
}

#[test]
fn a_session_without_history_is_not_found() {
    let (_temp, base) = workspace();

    let (status, ..) = history_reply(&base, &format!("{ROUTE_PREFIX}session-a"));

    assert_eq!(status, 404);
}

#[test]
fn an_id_that_could_leave_the_history_directory_is_not_found() {
    let (_temp, base) = workspace();
    for id in ["..", "a%2Fb", "a/b", "", "has space"] {
        let (status, ..) = history_reply(&base, &format!("{ROUTE_PREFIX}{id}"));
        assert_eq!(status, 404, "{id:?}");
    }
}

#[test]
fn a_history_over_the_size_limit_is_not_served() {
    let (_temp, base) = workspace();
    let root = work_root(&base);
    std::fs::create_dir_all(root.join("terminal-history")).unwrap();
    std::fs::write(
        root.join("terminal-history/session-a.terminal"),
        vec![b'x'; MAX_HISTORY_BYTES + 1],
    )
    .unwrap();

    let (status, ..) = history_reply(&base, &format!("{ROUTE_PREFIX}session-a"));

    assert_eq!(status, 404);
}

#[test]
fn history_is_not_served_unless_terminals_are_enabled() {
    if skip_without_loopback("history_is_not_served_unless_terminals_are_enabled") {
        return;
    }
    let (_temp, base) = workspace();
    write_history(&work_root(&base), "session-a", b"screen").unwrap();
    let (port, running) = start(base);

    let response = request(port, &history_request(port, "session-a", None, None));

    stop(running);
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}

#[test]
fn history_needs_the_token_cookie() {
    if skip_without_loopback("history_needs_the_token_cookie") {
        return;
    }
    let (_temp, base) = workspace();
    write_history(&work_root(&base), "session-a", b"screen").unwrap();
    let (port, running, _) = start_with(base, terminal_options());

    let none = request(port, &history_request(port, "session-a", None, None));
    let wrong = request(
        port,
        &history_request(port, "session-a", Some("x=wrong"), None),
    );

    stop(running);
    assert!(none.starts_with("HTTP/1.1 401"), "{none}");
    assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
}

#[test]
fn history_refuses_a_foreign_origin() {
    if skip_without_loopback("history_refuses_a_foreign_origin") {
        return;
    }
    let (_temp, base) = workspace();
    write_history(&work_root(&base), "session-a", b"screen").unwrap();
    let (port, running, _) = start_with(base, terminal_options());

    let response = request(
        port,
        &history_request(
            port,
            "session-a",
            Some(&token_cookie(port)),
            Some("http://evil.example"),
        ),
    );

    stop(running);
    assert!(response.starts_with("HTTP/1.1 403"), "{response}");
}

#[test]
fn history_with_the_token_cookie_is_served() {
    if skip_without_loopback("history_with_the_token_cookie_is_served") {
        return;
    }
    let (_temp, base) = workspace();
    write_history(&work_root(&base), "session-a", b"screen text\n").unwrap();
    let (port, running, _) = start_with(base, terminal_options());

    let response = request(
        port,
        &history_request(port, "session-a", Some(&token_cookie(port)), None),
    );

    stop(running);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Content-Type: application/octet-stream"));
    assert_eq!(body(&response), "screen text\n");
}
