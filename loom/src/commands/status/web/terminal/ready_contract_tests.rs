//! Contract of stage runtime-and-dashboard-server: an attached terminal sends
//! exactly one `{"ready":true}` text frame before any terminal output.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use crate::process::sandbox_probe::{
    loopback_bindable, skip_unless, skip_without_group_signals, skip_without_pty,
};

use super::protocol::{Mode, WindowSize};
use super::pty::PtyChild;
use super::upgrade;

const TEST: &str = "the_ready_frame_precedes_terminal_output";
const READY: &str = r#"{"ready":true}"#;

/// The next data frame, skipping control frames and read timeouts until `deadline`.
fn next_data(socket: &mut WebSocket<TcpStream>, deadline: Instant) -> Message {
    while Instant::now() < deadline {
        match socket.read() {
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(message) => return message,
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => panic!("reading the terminal socket: {error}"),
        }
    }
    panic!("no data frame within the deadline");
}

/// Attaches one client to a PTY child that prints `OUT` and then sleeps.
fn spawn_attached(listener: TcpListener, running: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept client");
        let socket = tungstenite::accept(stream).expect("accept WebSocket");
        let mut command = Command::new("sh");
        command.args(["-c", "printf OUT; sleep 30"]);
        let child = PtyChild::spawn(&mut command, WindowSize { cols: 80, rows: 24 })
            .expect("spawn PTY child");
        upgrade::run_attached(socket, child, Mode::View, running.as_ref());
    })
}

#[test]
fn the_ready_frame_precedes_terminal_output() {
    if skip_unless(
        loopback_bindable(),
        TEST,
        "loopback sockets are unavailable",
    ) || skip_without_pty(TEST)
        || skip_without_group_signals(TEST)
    {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("listener address").port();
    let running = Arc::new(AtomicBool::new(true));
    let server = spawn_attached(listener, Arc::clone(&running));
    let request = format!("ws://127.0.0.1:{port}/")
        .into_client_request()
        .expect("request");
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect client");
    let (mut client, _) = tungstenite::client(request, stream).expect("handshake");
    let timeout = Some(Duration::from_millis(100));
    client
        .get_mut()
        .set_read_timeout(timeout)
        .expect("read timeout");
    let deadline = Instant::now() + Duration::from_secs(10);
    let first = next_data(&mut client, deadline);
    assert!(
        matches!(&first, Message::Text(text) if text.as_str() == READY),
        "the first frame must be the ready frame, got {first:?}"
    );
    let mut output = Vec::new();
    while !output.windows(3).any(|w| w == b"OUT") {
        match next_data(&mut client, deadline) {
            Message::Binary(bytes) => output.extend_from_slice(&bytes),
            other => panic!("expected terminal output after ready, got {other:?}"),
        }
    }
    running.store(false, Ordering::SeqCst);
    drop(client);
    server.join().expect("bridge thread");
}
