//! Socket/loopback dashboard tests: each of these binds or connects to a
//! real TCP listener, so they are skipped in sandboxes without loopback TCP.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tungstenite::client::IntoClientRequest;

use super::{
    assert_security_headers, body, request, skip_without_loopback, start, stop, workspace,
};
use crate::commands::status::web::broadcast::Broadcaster;
use crate::commands::status::web::model::{DaemonState, SnapshotSource, WebSnapshot};
use crate::commands::status::web::{assets, http};
use crate::process::sandbox_probe::{loopback_bindable, skip_unless};

fn skip_without_assets(test_name: &str) -> bool {
    skip_unless(
        loopback_bindable() && !assets::WEB_ASSETS.is_empty(),
        test_name,
        "embedded assets are absent or loopback TCP is unavailable",
    )
}

fn split_request(port: u16, first: &str, headers: &str) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect split request");
    stream
        .write_all(first.as_bytes())
        .expect("write request line");
    stream.flush().expect("flush request line");
    thread::sleep(Duration::from_millis(20));
    stream
        .write_all(headers.as_bytes())
        .expect("write request headers");
    stream.flush().expect("flush request headers");
    thread::sleep(Duration::from_millis(20));
    stream.write_all(b"\r\n").expect("finish request head");
    stream.flush().expect("flush request head");
    stream
}

fn read_http_head(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).expect("read response head");
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).expect("response head is UTF-8")
}

fn read_websocket_text(stream: &mut TcpStream) -> String {
    let mut head = [0_u8; 2];
    stream
        .read_exact(&mut head)
        .expect("read WebSocket frame head");
    assert_eq!(head[0] & 0x0f, 1, "expected a text frame");
    let length = match head[1] & 0x7f {
        value @ 0..=125 => usize::from(value),
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).expect("read frame length");
            usize::from(u16::from_be_bytes(bytes))
        }
        _ => {
            let mut bytes = [0_u8; 8];
            stream.read_exact(&mut bytes).expect("read frame length");
            usize::try_from(u64::from_be_bytes(bytes)).expect("frame length fits usize")
        }
    };
    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .expect("read WebSocket text");
    String::from_utf8(payload).expect("WebSocket text is UTF-8")
}

#[test]
fn read_head_rejects_oversized() {
    if skip_without_loopback("read_head_rejects_oversized") {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("listener address").port();
    let writer = thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect listener");
        let request = format!(
            "GET / HTTP/1.1\r\nX-Long: {}",
            "x".repeat(http::MAX_HEAD_BYTES)
        );
        stream
            .write_all(request.as_bytes())
            .expect("write oversized request");
    });
    let (mut stream, _) = listener.accept().expect("accept client");
    assert!(http::read_head(&mut stream).is_err());
    writer.join().expect("writer thread");
}

#[test]
fn api_status_returns_snapshot_json() {
    if skip_without_loopback("api_status_returns_snapshot_json") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let response = request(port, "GET /api/status HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stop(running);
    assert!(response.starts_with("HTTP/1.1 200"));
    let snapshot: WebSnapshot = serde_json::from_str(body(&response)).expect("snapshot JSON");
    assert_eq!(snapshot.source, SnapshotSource::Files);
    assert_eq!(snapshot.daemon, DaemonState::NotRunning);
}

#[test]
fn api_status_rejects_foreign_origin() {
    if skip_without_loopback("api_status_rejects_foreign_origin") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let response = request(
        port,
        "GET /api/status HTTP/1.1\r\nHost: localhost\r\nOrigin: http://evil.example\r\n\r\n",
    );
    stop(running);
    assert!(response.starts_with("HTTP/1.1 403"));
    assert_security_headers(&response);
}

#[test]
fn index_reports_missing_assets() {
    if skip_unless(
        loopback_bindable() && assets::WEB_ASSETS.is_empty(),
        "index_reports_missing_assets",
        "embedded assets are present or loopback TCP is unavailable",
    ) {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let response = request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stop(running);
    assert!(response.starts_with("HTTP/1.1 503"));
    assert_security_headers(&response);
}

#[test]
fn index_serves_embedded_page() {
    if skip_without_assets("index_serves_embedded_page") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let index = request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let fallback = request(
        port,
        "GET /stages/anything HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    let missing = request(
        port,
        "GET /assets/nope.js HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    stop(running);
    assert!(index.starts_with("HTTP/1.1 200"));
    assert!(body(&index).contains("<div id=\"root\">"));
    assert!(fallback.starts_with("HTTP/1.1 200"));
    assert_eq!(body(&fallback), body(&index));
    assert!(missing.starts_with("HTTP/1.1 404"));
}

#[test]
fn websocket_delivers_a_snapshot() {
    if skip_without_loopback("websocket_delivers_a_snapshot") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let url = format!("ws://127.0.0.1:{port}/ws");
    let request = url
        .as_str()
        .into_client_request()
        .expect("build WebSocket request");
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect WebSocket");
    let (mut socket, _) =
        tungstenite::client(request, stream).expect("complete WebSocket handshake");
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set WebSocket timeout");
    let frame = socket.read().expect("read snapshot frame");
    let snapshot: WebSnapshot =
        serde_json::from_str(frame.to_text().expect("text frame")).expect("snapshot JSON");
    assert_eq!(snapshot.source, SnapshotSource::Files);
    let _ = socket.close(None);
    stop(running);
}

#[test]
fn websocket_rejects_foreign_origin() {
    if skip_without_loopback("websocket_rejects_foreign_origin") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let response = request(
        port,
        "GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nOrigin: http://evil.example\r\n\r\n",
    );
    stop(running);
    assert!(response.starts_with("HTTP/1.1 403"));
}

#[test]
fn broadcaster_publishes_file_snapshot_without_daemon() {
    if skip_without_loopback("broadcaster_publishes_file_snapshot_without_daemon") {
        return;
    }
    let (_temp, base) = workspace();
    let running = Arc::new(AtomicBool::new(true));
    let broadcaster = Broadcaster::spawn(base, running.clone());
    let frame = broadcaster
        .subscribe()
        .recv_timeout(Duration::from_secs(5))
        .expect("file frame");
    stop(running);
    let snapshot: WebSnapshot = serde_json::from_str(&frame).expect("snapshot JSON");
    assert_eq!(snapshot.source, SnapshotSource::Files);
}

#[test]
fn handshake_survives_a_split_head() {
    if skip_without_loopback("handshake_survives_a_split_head") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let headers = concat!(
        "Host: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n",
        "Sec-WebSocket-Version: 13\r\n",
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
    );
    let mut stream = split_request(port, "GET /ws HTTP/1.1\r\n", headers);
    let response = read_http_head(&mut stream);
    assert!(response.starts_with("HTTP/1.1 101"));
    let snapshot: WebSnapshot =
        serde_json::from_str(&read_websocket_text(&mut stream)).expect("snapshot JSON");
    assert_eq!(snapshot.source, SnapshotSource::Files);
    stop(running);
}

#[test]
fn split_head_get_is_routed_normally() {
    if skip_without_assets("split_head_get_is_routed_normally") {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running) = start(base);
    let mut stream = split_request(port, "GET / HTTP/1.1\r\n", "Host: localhost\r\n");
    let response = read_http_head(&mut stream);
    stop(running);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_security_headers(&response);
}
