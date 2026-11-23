//! What a browser sees once the upgrade is admitted: the ready frame ahead of
//! any terminal output, and the typed close frames of a failed attach. The
//! terminal origin gate shares the dashboard's strict origin grammar.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use crate::commands::status::web::tests::skip_without_loopback;
use crate::process::sandbox_probe::{skip_without_group_signals, skip_without_pty};

use super::super::protocol::{Mode, WindowSize, CLOSE_ATTACH_FAILED, READY_FRAME};
use super::super::pty::PtyChild;
use super::super::upgrade::{
    close_with, origin_matches_host, run_attached, RESOLVE_FAILED, SPAWN_FAILED,
};

fn connect(port: u16) -> WebSocket<TcpStream> {
    let request = format!("ws://127.0.0.1:{port}/")
        .into_client_request()
        .expect("build WebSocket request");
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect client");
    let (mut client, _) = tungstenite::client(request, stream).expect("client handshake");
    client
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set client read timeout");
    client
}

/// The next data frame, skipping pings and read timeouts until `deadline`.
fn next_data(client: &mut WebSocket<TcpStream>, deadline: Instant) -> Message {
    while Instant::now() < deadline {
        match client.read() {
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(message) => return message,
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => panic!("reading the terminal socket: {error}"),
        }
    }
    panic!("no data frame within the deadline");
}

fn serve_output(listener: TcpListener, running: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept client");
        let socket = tungstenite::accept(stream).expect("accept WebSocket");
        let mut command = Command::new("sh");
        command.args(["-c", "printf OUT; sleep 30"]);
        let child = PtyChild::spawn(&mut command, WindowSize { cols: 80, rows: 24 })
            .expect("spawn PTY child");
        run_attached(socket, child, Mode::View, running.as_ref());
    })
}

#[test]
fn the_ready_frame_precedes_terminal_output() {
    let name = "the_ready_frame_precedes_terminal_output";
    if skip_without_loopback(name) || skip_without_pty(name) || skip_without_group_signals(name) {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("listener address").port();
    let running = Arc::new(AtomicBool::new(true));
    let server = serve_output(listener, Arc::clone(&running));
    let mut client = connect(port);
    let deadline = Instant::now() + Duration::from_secs(10);
    let first = next_data(&mut client, deadline);
    assert!(
        matches!(&first, Message::Text(text) if text.as_str() == READY_FRAME),
        "the first frame must be the ready frame, got {first:?}"
    );
    let mut output = Vec::new();
    while !output.windows(3).any(|part| part == b"OUT") {
        match next_data(&mut client, deadline) {
            Message::Binary(bytes) => output.extend_from_slice(&bytes),
            other => panic!("expected terminal output after the ready frame, got {other:?}"),
        }
    }
    running.store(false, Ordering::SeqCst);
    drop(client);
    server.join().expect("bridge thread");
}

#[test]
fn close_with_sends_the_typed_close_frame() {
    if skip_without_loopback("close_with_sends_the_typed_close_frame") {
        return;
    }
    for reason in [RESOLVE_FAILED, SPAWN_FAILED] {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("listener address").port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept client");
            let mut socket = tungstenite::accept(stream).expect("accept WebSocket");
            close_with(&mut socket, CLOSE_ATTACH_FAILED, reason);
        });
        let mut client = connect(port);
        let deadline = Instant::now() + Duration::from_secs(10);
        let frame = match next_data(&mut client, deadline) {
            Message::Close(Some(frame)) => frame,
            other => panic!("expected a close frame, got {other:?}"),
        };
        server.join().expect("server thread");
        assert_eq!(frame.code, CloseCode::Library(4010));
        assert_eq!(frame.reason.as_str(), reason);
    }
}

#[test]
fn terminal_origin_with_a_path_or_userinfo_suffix_is_refused() {
    let host = Some("127.0.0.1:7373");
    for origin in [
        "http://127.0.0.1:7373/",
        "http://127.0.0.1:7373/path",
        "http://127.0.0.1:7373?q=1",
        "http://127.0.0.1:7373#frag",
        "http://user@127.0.0.1:7373",
        "http://127.0.0.1:7373@evil.example",
    ] {
        assert!(!origin_matches_host(Some(origin), host), "{origin}");
    }
    assert!(origin_matches_host(Some("http://127.0.0.1:7373"), host));
}
