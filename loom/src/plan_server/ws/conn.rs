//! The loop that serves one admitted planning WebSocket connection.

use std::io;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Error, Message, Utf8Bytes, WebSocket};

use super::hub::PlanHub;
use crate::plan_server::ids::ConnCtx;
use crate::plan_server::wire::message::ClientMessage;

/// How long a blocked send or an idle read may hold the connection thread.
const SOCKET_TIMEOUT: Duration = Duration::from_millis(250);

/// Outbound silence after which a protocol ping is sent, so an idle
/// connection through a proxy or NAT is not reaped.
const KEEPALIVE: Duration = Duration::from_secs(10);

type Socket = WebSocket<TcpStream>;

/// Complete the handshake on an admitted stream, then serve it: the hub's
/// `on_open` frames first, then queued frames out and client messages in,
/// until the client leaves, the hub drops the connection, or `running`
/// clears. Inbound messages and frames are capped at `max_inbound` bytes; a
/// larger one closes the connection. A text frame that is not a
/// [`ClientMessage`], and every binary frame, goes to `on_bad_frame` and the
/// connection stays open.
///
/// Fails only when the handshake or the socket setup fails, before the hub
/// sees the connection. Once registered, the connection is unregistered and
/// the `on_close` hooks run on every exit path, a panic included.
pub fn run_connection(
    stream: TcpStream,
    conn: ConnCtx,
    hub: &PlanHub,
    max_inbound: usize,
    running: &AtomicBool,
    on_message: &dyn Fn(&ConnCtx, ClientMessage),
    on_bad_frame: &dyn Fn(&ConnCtx, &str),
) -> io::Result<()> {
    let mut socket = accept(stream, max_inbound)?;
    let rx = hub.register(&conn);
    let registration = Registration { hub, conn };
    let conn = &registration.conn;
    let mut last_write = Instant::now();
    for frame in hub.open_frames(conn) {
        if write(&mut socket, &frame, &mut last_write).is_err() {
            return Ok(());
        }
    }
    while running.load(Ordering::SeqCst) {
        if !send_pending(&mut socket, &rx, &mut last_write)
            || !keep_alive(&mut socket, &mut last_write)
        {
            return Ok(());
        }
        match read_one(&mut socket) {
            Inbound::Idle => {}
            Inbound::Text(text) => match serde_json::from_str::<ClientMessage>(text.as_str()) {
                Ok(msg) => on_message(conn, msg),
                Err(error) => on_bad_frame(conn, &error.to_string()),
            },
            Inbound::Binary => on_bad_frame(conn, "binary frames are not accepted"),
            Inbound::Closed => return Ok(()),
        }
    }
    // Stopping: flush what is queued (a final `server_stopping`) and say goodbye.
    send_pending(&mut socket, &rx, &mut last_write);
    let _ = socket.close(None);
    let _ = socket.flush();
    Ok(())
}

/// Unregisters the connection and runs the `on_close` hooks when dropped.
struct Registration<'a> {
    hub: &'a PlanHub,
    conn: ConnCtx,
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.hub.unregister(self.conn.id);
        self.hub.closed(&self.conn);
    }
}

fn accept(stream: TcpStream, max_inbound: usize) -> io::Result<Socket> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(max_inbound))
        .max_frame_size(Some(max_inbound));
    let mut socket = tungstenite::accept_with_config(stream, Some(config)).map_err(|error| {
        io::Error::other(format!("planning WebSocket handshake failed: {error}"))
    })?;
    // Without a write timeout a peer that stops reading parks this thread in
    // `send` for good once its receive window fills.
    let peer = socket.get_mut();
    peer.set_read_timeout(Some(SOCKET_TIMEOUT))?;
    peer.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    Ok(socket)
}

fn write(socket: &mut Socket, frame: &str, last_write: &mut Instant) -> tungstenite::Result<()> {
    socket.send(Message::text(frame))?;
    *last_write = Instant::now();
    Ok(())
}

/// Write every queued frame. False when the socket failed or the hub
/// dropped the connection.
fn send_pending(socket: &mut Socket, rx: &Receiver<Arc<String>>, last_write: &mut Instant) -> bool {
    loop {
        match rx.try_recv() {
            Ok(frame) => {
                if write(socket, &frame, last_write).is_err() {
                    return false;
                }
            }
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => return false,
        }
    }
}

fn keep_alive(socket: &mut Socket, last_write: &mut Instant) -> bool {
    if last_write.elapsed() < KEEPALIVE {
        return true;
    }
    *last_write = Instant::now();
    socket.send(Message::Ping(Default::default())).is_ok()
}

enum Inbound {
    Idle,
    Text(Utf8Bytes),
    Binary,
    Closed,
}

fn read_one(socket: &mut Socket) -> Inbound {
    match socket.read() {
        Ok(Message::Text(text)) => Inbound::Text(text),
        Ok(Message::Binary(_)) => Inbound::Binary,
        Ok(Message::Close(_)) => Inbound::Closed,
        Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => Inbound::Idle,
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Inbound::Idle
        }
        Err(Error::Capacity(error)) => {
            tracing::debug!("planning WebSocket message over the inbound limit: {error}");
            let frame = CloseFrame {
                code: CloseCode::Size,
                reason: "message too large".into(),
            };
            let _ = socket.close(Some(frame));
            let _ = socket.flush();
            Inbound::Closed
        }
        Err(Error::ConnectionClosed | Error::AlreadyClosed) => Inbound::Closed,
        Err(error) => {
            tracing::debug!("planning WebSocket read failed: {error}");
            Inbound::Closed
        }
    }
}
