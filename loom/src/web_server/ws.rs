//! WebSocket delivery of text frames from a [`super::hub::Hub`] subscription.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error, Message};

use super::limits::Limits;

/// How long a blocked send or an idle read may hold the connection thread.
const SOCKET_TIMEOUT: Duration = Duration::from_millis(250);

/// Complete a peek-preserved handshake, then stream text frames until the
/// client leaves or `running` clears. Inbound messages and frames are capped
/// at [`Limits::max_inbound_bytes`].
///
/// This sends only what the subscription yields. Liveness is the producer's
/// job: the dashboard publishes a `tick` message on every collect, which is how
/// a client tells a quiet tree from a half-open connection (browser JS cannot
/// observe protocol pings). The client's watchdog (web/src/api/ws.ts) must stay
/// well above the producer's collect interval.
pub(crate) fn handle(
    stream: TcpStream,
    rx: Receiver<Arc<String>>,
    running: &AtomicBool,
    limits: &Limits,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(limits.max_inbound_bytes()))
        .max_frame_size(Some(limits.max_inbound_bytes()));
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
    while running.load(Ordering::SeqCst) {
        if !send_pending(&mut socket, &rx) {
            return;
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
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(frame) => {
                if socket.send(Message::text(frame.as_str())).is_err() {
                    return false;
                }
            }
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }
    }
}
