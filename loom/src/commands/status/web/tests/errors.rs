//! Wire-level tests for the error responses the dashboard writes itself.
//!
//! Every one of these paths answers a request whose bytes were deliberately
//! left unread, so each asserts the response arrives *complete* rather than
//! being destroyed by an RST on close. Asserting on the response builder
//! instead would pass either way.

use std::time::Duration;

use super::{
    assert_security_headers, body, request, request_with_timeout, skip_without_loopback, start,
    stop, workspace,
};
use crate::commands::status::web::http::MAX_HEAD_BYTES;

/// Read the `Content-Length` a response advertises.
fn content_length(response: &str) -> usize {
    response
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.trim().parse().ok())
        .expect("response carries a Content-Length")
}

#[test]
fn post_with_a_body_is_answered_with_a_readable_405() {
    if skip_without_loopback("post_with_a_body_is_answered_with_a_readable_405") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    // Larger than one `read_head` chunk, so bytes are certain to be unread
    // when the 405 is written.
    let payload = "x".repeat(8 * 1024);
    let response = request(
        port,
        &format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{payload}",
            payload.len()
        ),
    );
    stop(running);
    assert!(response.starts_with("HTTP/1.1 405"), "{response}");
    assert_security_headers(&response);
    assert_eq!(body(&response), "GET required");
}

#[test]
fn an_oversized_header_block_is_answered_with_a_readable_431() {
    if skip_without_loopback("an_oversized_header_block_is_answered_with_a_readable_431") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    // Never terminated, so the head can only ever overflow the budget.
    let response = request(
        port,
        &format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nX-Long: {}",
            "y".repeat(MAX_HEAD_BYTES + 4096)
        ),
    );
    stop(running);
    assert!(response.starts_with("HTTP/1.1 431"), "{response}");
    assert_security_headers(&response);
}

#[test]
fn a_silent_client_is_answered_with_a_408() {
    if skip_without_loopback("a_silent_client_is_answered_with_a_408") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let response = request_with_timeout(port, "", Duration::from_secs(20));
    stop(running);
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    assert_security_headers(&response);
}

#[test]
fn head_omits_the_body_but_keeps_the_length() {
    if skip_without_loopback("head_omits_the_body_but_keeps_the_length") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let head = request(port, "HEAD / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let get = request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stop(running);
    assert_eq!(
        head.lines().next(),
        get.lines().next(),
        "HEAD and GET must agree on the status line"
    );
    assert!(body(&head).is_empty(), "HEAD response carried a body");
    assert_security_headers(&head);
    let length = content_length(&head);
    assert_eq!(length, body(&get).len());
    assert!(length > 0, "HEAD must report the length a GET would send");
}
