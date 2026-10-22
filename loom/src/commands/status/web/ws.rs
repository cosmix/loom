//! WebSocket delivery for dashboard snapshots.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error, Message};

use super::limits::MAX_INBOUND_BYTES;

/// How long a blocked send or an idle read may hold the connection thread.
const SOCKET_TIMEOUT: Duration = Duration::from_millis(250);

/// Silence after which the last frame is sent again. The broadcaster drops
/// unchanged frames, so a quiet tree sends nothing, and a client cannot tell
/// that from a half-open connection. Browser JS cannot observe protocol pings,
/// hence a text frame. The client's watchdog (web/src/api/ws.ts) must stay well
/// above this.
const KEEPALIVE: Duration = Duration::from_secs(10);

/// Complete a peek-preserved handshake, then stream snapshot text frames until
/// the client leaves or `running` clears.
pub fn handle(stream: TcpStream, rx: Receiver<Arc<String>>, running: &AtomicBool) {
    handle_with_keepalive(stream, rx, running, KEEPALIVE);
}

pub(super) fn handle_with_keepalive(
    stream: TcpStream,
    rx: Receiver<Arc<String>>,
    running: &AtomicBool,
    keepalive: Duration,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_INBOUND_BYTES))
        .max_frame_size(Some(MAX_INBOUND_BYTES));
    let Ok(mut socket) = tungstenite::accept_with_config(stream, Some(config)) else {
        tracing::warn!("dashboard WebSocket handshake failed");
        return;
    };
    // Without a write timeout a peer that stops reading parks this thread in
    // `send` for good once its receive window fills.
    let peer = socket.get_mut();
    if peer.set_read_timeout(Some(SOCKET_TIMEOUT)).is_err()
        || peer.set_write_timeout(Some(SOCKET_TIMEOUT)).is_err()
    {
        return;
    }
    let mut last_sent: Option<(Arc<String>, Instant)> = None;
    while running.load(Ordering::SeqCst) {
        if !send_pending(&mut socket, &rx, &mut last_sent) {
            return;
        }
        if let Some((frame, at)) = &mut last_sent {
            if at.elapsed() >= keepalive {
                if socket.send(Message::text(frame.as_str())).is_err() {
                    return;
                }
                *at = Instant::now();
            }
        }
        match socket.read() {
            Ok(Message::Close(_)) | Err(Error::ConnectionClosed | Error::AlreadyClosed) => return,
            Ok(_) => {}
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

fn send_pending(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    rx: &Receiver<Arc<String>>,
    last_sent: &mut Option<(Arc<String>, Instant)>,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(frame) => {
                if socket.send(Message::text(frame.as_str())).is_err() {
                    return false;
                }
                *last_sent = Some((frame, Instant::now()));
            }
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }
    }
}
