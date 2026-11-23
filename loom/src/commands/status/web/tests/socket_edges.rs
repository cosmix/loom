//! Socket tests for the dashboard's admission edges: the connection cap, and
//! the `Host`/`Origin` rule on `/ws`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use super::{body, request, skip_without_loopback, start, stop, workspace};
use crate::web_server::limits::MAX_CONNECTIONS;

/// Open `MAX_CONNECTIONS` silent connections. They hold their slots for the
/// whole head timeout, and the accept loop takes connections in the order they
/// were opened, so the next request always arrives at a full pool.
fn hold_every_slot(port: u16) -> Vec<TcpStream> {
    (0..MAX_CONNECTIONS)
        .map(|index| {
            TcpStream::connect(("127.0.0.1", port))
                .unwrap_or_else(|error| panic!("hold connection {index}: {error}"))
        })
        .collect()
}

/// A `/ws` upgrade request whose `Host` is `host` and whose `Origin` is
/// `origin`, when there is one.
fn upgrade(host: &str, origin: Option<&str>) -> String {
    let origin = origin.map_or_else(String::new, |origin| format!("Origin: {origin}\r\n"));
    format!(
        "GET /ws HTTP/1.1\r\nHost: {host}\r\n{origin}Upgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    )
}

#[test]
fn a_connection_past_the_cap_is_answered_503() {
    if skip_without_loopback("a_connection_past_the_cap_is_answered_503") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);

    let held = hold_every_slot(port);
    let response = request(port, "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    drop(held);
    stop(running);

    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert!(
        body(&response).contains("connection limit reached"),
        "{response}"
    );
}

#[test]
fn a_rejected_connection_does_not_wait_for_a_trickling_client() {
    if skip_without_loopback("a_rejected_connection_does_not_wait_for_a_trickling_client") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let held = hold_every_slot(port);

    // Each trickler is refused at the cap. A refusal that drained its client
    // in place would spend up to 300 ms of the accept loop on every one.
    let tricklers = (0..6)
        .map(|_| {
            thread::spawn(move || {
                let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
                    return;
                };
                for _ in 0..75 {
                    if stream.write_all(b"x").is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(40));
                }
            })
        })
        .collect::<Vec<_>>();
    thread::sleep(Duration::from_millis(100));

    let mut probe = TcpStream::connect(("127.0.0.1", port)).expect("connect probe");
    probe
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write probe request");
    probe
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("set probe timeout");
    let mut response = Vec::new();
    let read = probe.read_to_end(&mut response);

    for trickler in tricklers {
        trickler.join().expect("join trickler");
    }
    drop(held);
    stop(running);

    assert!(read.is_ok(), "the probe waited on the tricklers: {read:?}");
    assert!(
        response.starts_with(b"HTTP/1.1 503"),
        "{}",
        String::from_utf8_lossy(&response)
    );
}

#[test]
fn websocket_from_another_localhost_port_is_refused() {
    if skip_without_loopback("websocket_from_another_localhost_port_is_refused") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let host = format!("127.0.0.1:{port}");
    let foreign = request(port, &upgrade(&host, Some("http://localhost:3000")));
    let absent = request(port, &upgrade(&host, None));
    stop(running);

    for response in [foreign, absent] {
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        assert_eq!(body(&response), "origin not allowed");
    }
}

#[test]
fn websocket_through_a_forwarded_port_is_accepted() {
    if skip_without_loopback("websocket_through_a_forwarded_port_is_accepted") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    // An SSH tunnel (`ssh -L 8080:127.0.0.1:<port>`): neither header names the
    // server's own port, and the two agree with each other.
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream
        .write_all(upgrade("localhost:8080", Some("http://localhost:8080")).as_bytes())
        .expect("write upgrade");
    let mut head = [0_u8; 12];
    stream.read_exact(&mut head).expect("read status line");
    drop(stream);
    stop(running);

    assert_eq!(&head, b"HTTP/1.1 101");
}
