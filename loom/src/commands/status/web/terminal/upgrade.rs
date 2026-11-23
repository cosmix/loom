//! Admission, resolution, and tmux hand-off for one terminal WebSocket.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use crate::orchestrator::terminal::tmux::viewer::is_plain_identifier;

use super::bridge;
use super::protocol::{CLOSE_ATTACH_FAILED, CLOSE_BUSY, CLOSE_SERVER_STOPPING, READY_FRAME};
use super::pty::PtyChild;
use super::resolve::{self, attach_args, Target};
use super::{Mode, WindowSize};
use crate::commands::status::web::{TerminalLane, MAX_INBOUND_BYTES};
use crate::web_server::access::{same_origin, AccessPolicy};
use crate::web_server::connection::fail;
use crate::web_server::http::RequestHead;
use crate::web_server::limits::{acquire_terminal_slot, Limits, Slot};
use crate::web_server::token;

/// Close reasons for a failed attach. `web/src/api/terminal.ts` keys on the
/// close code, so these only need to read well.
pub(super) const RESOLVE_FAILED: &str = "could not resolve the stage session";
pub(super) const SPAWN_FAILED: &str = "could not start the tmux attach client";

/// What an admitted upgrade does once the handshake is complete.
enum Admission<'a> {
    /// Attach a tmux client; the slot is held until the bridge returns.
    Attach {
        stage_id: &'a str,
        control: bool,
        slot: Slot,
    },
    /// Close at once, holding no slot and spawning nothing. Closing after the
    /// handshake is what lets the browser tell this from a bad cookie.
    Close { code: u16, reason: &'static str },
}

impl TerminalLane {
    pub(in crate::commands::status::web) fn from_token(token: String, port: u16) -> Self {
        Self {
            token,
            cookie_name: Self::cookie_name(port),
        }
    }

    pub(in crate::commands::status::web) fn cookie_name(port: u16) -> String {
        token::cookie_name(port)
    }

    pub(in crate::commands::status::web) fn cookie_matches(&self, cookie: Option<&str>) -> bool {
        token::matches(token::cookie_token(cookie, &self.cookie_name), &self.token)
    }
}

/// Upgrade one validated terminal request, or answer its pre-upgrade refusal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_upgrade(
    mut stream: TcpStream,
    head: &RequestHead,
    base: &Path,
    lane: Option<&TerminalLane>,
    running: &AtomicBool,
    limits: &Arc<Limits>,
    policy: &AccessPolicy,
    local: SocketAddr,
) {
    let Some(admission) = admit(&mut stream, head, lane, running, limits, policy, local) else {
        return;
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_INBOUND_BYTES))
        .max_frame_size(Some(MAX_INBOUND_BYTES));
    let Ok(mut socket) = tungstenite::accept_with_config(stream, Some(config)) else {
        tracing::warn!("dashboard terminal WebSocket handshake failed");
        return;
    };
    match admission {
        Admission::Close { code, reason } => close_with(&mut socket, code, reason),
        Admission::Attach {
            stage_id,
            control,
            slot: _slot,
        } => start_bridge(socket, stage_id, base, control, running),
    }
}

fn admit<'a>(
    stream: &mut TcpStream,
    head: &'a RequestHead,
    lane: Option<&TerminalLane>,
    running: &AtomicBool,
    limits: &Arc<Limits>,
    policy: &AccessPolicy,
    local: SocketAddr,
) -> Option<Admission<'a>> {
    let (stage_id, control) = authorize(stream, head, lane, policy, local)?;
    // Both refusals below complete the handshake first (see `handle_upgrade`):
    // an HTTP failure reads in the browser as a rejected cookie. A stopping
    // server is checked before the slot so it takes none.
    if !running.load(Ordering::SeqCst) {
        return Some(Admission::Close {
            code: CLOSE_SERVER_STOPPING,
            reason: "server stopping",
        });
    }
    let Some(slot) = acquire_terminal_slot(limits) else {
        return Some(Admission::Close {
            code: CLOSE_BUSY,
            reason: "terminal limit reached",
        });
    };
    Some(Admission::Attach {
        stage_id,
        control,
        slot,
    })
}

/// The pre-upgrade gates: host, lane, origin, cookie, then the route. A
/// failed gate answers its HTTP refusal on `stream` and returns `None`.
fn authorize<'a>(
    stream: &mut TcpStream,
    head: &'a RequestHead,
    lane: Option<&TerminalLane>,
    policy: &AccessPolicy,
    local: SocketAddr,
) -> Option<(&'a str, bool)> {
    // Re-asserted here rather than trusted to the caller: `web_server::serve`
    // checks this ahead of routing today, but `handle_upgrade` is
    // `pub(crate)` and this lane is a keystroke-injection surface, so the
    // gate belongs to the lane it guards rather than to whichever caller
    // happens to sit above it. Policy-aware so a remote bind's stricter
    // socket-identity check applies here too.
    if !policy.host_allowed(local, head) {
        fail(stream, 403, "Forbidden", b"host not allowed");
        return None;
    }
    let Some(lane) = lane else {
        fail(stream, 404, "Not Found", b"not found");
        return None;
    };
    if !origin_matches_host(head.origin.as_deref(), head.host.as_deref()) {
        fail(stream, 403, "Forbidden", b"origin not allowed");
        return None;
    }
    if !lane.cookie_matches(head.cookie.as_deref()) {
        fail(stream, 401, "Unauthorized", b"dashboard token required");
        return None;
    }
    let route = terminal_path(&head.path);
    if route.is_none() {
        fail(stream, 404, "Not Found", b"not found");
    }
    route
}

fn start_bridge(
    mut socket: WebSocket<TcpStream>,
    stage_id: &str,
    base: &Path,
    control: bool,
    running: &AtomicBool,
) {
    let target = match resolve::resolve_target(stage_id, base) {
        Ok(Ok(target)) => target,
        Ok(Err(refusal)) => {
            return close_with(&mut socket, refusal.close_code(), refusal.reason());
        }
        Err(error) => {
            tracing::warn!("dashboard terminal target resolution failed: {error}");
            return close_with(&mut socket, CLOSE_ATTACH_FAILED, RESOLVE_FAILED);
        }
    };
    let mut command = tmux_command(&target, control);
    let Ok(child) = PtyChild::spawn(
        &mut command,
        WindowSize {
            cols: 120,
            rows: 36,
        },
    ) else {
        tracing::warn!("dashboard terminal could not start tmux attach client");
        return close_with(&mut socket, CLOSE_ATTACH_FAILED, SPAWN_FAILED);
    };
    run_attached(socket, child, mode(control), running);
}

/// Tell the browser the terminal is attached, then pump. The ready frame goes
/// out before the bridge reads the PTY, so no output can precede it.
pub(super) fn run_attached(
    mut socket: WebSocket<TcpStream>,
    child: PtyChild,
    mode: Mode,
    running: &AtomicBool,
) {
    if socket.send(Message::text(READY_FRAME)).is_err() {
        child.shutdown();
        return;
    }
    bridge::run(socket, child, mode, running);
}

/// Require a browser origin that is `http(s)://` plus exactly `Host`, with no
/// path, query, userinfo or other suffix.
pub(super) fn origin_matches_host(origin: Option<&str>, host: Option<&str>) -> bool {
    matches!((origin, host), (Some(origin), Some(host)) if same_origin(origin, host))
}

pub(super) fn terminal_path(path: &str) -> Option<(&str, bool)> {
    let mut parts = path.strip_prefix("/ws/terminal/")?.split('/');
    let stage_id = parts.next()?;
    let control = match parts.next()? {
        "view" => false,
        "control" => true,
        _ => return None,
    };
    (parts.next().is_none() && is_plain_identifier(stage_id)).then_some((stage_id, control))
}

/// Close an accepted socket with a typed frame the browser can act on.
pub(super) fn close_with(socket: &mut WebSocket<TcpStream>, code: u16, reason: &str) {
    let frame = CloseFrame {
        code: CloseCode::Library(code),
        reason: reason.into(),
    };
    let _ = socket.close(Some(frame));
    let _ = socket.flush();
}

fn tmux_command(target: &Target, control: bool) -> Command {
    let mut command = Command::new("tmux");
    command.args(attach_args(target, mode(control)));
    command.env_remove("TMUX");
    command.env("TERM", "xterm-256color");
    command
}

fn mode(control: bool) -> Mode {
    if control {
        Mode::Control
    } else {
        Mode::View
    }
}
