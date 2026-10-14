//! Read-only terminal history: `GET /api/terminal-history/<session-id>` serves
//! the scrollback a tmux session left in `terminal-history/<session-id>.terminal`
//! when its server ended (`orchestrator::terminal::tmux::write_history`).
//!
//! The history is a session's terminal output, so it is held to the terminal
//! lane's gate: the dashboard must have been started with `--terminals`, and the
//! request must carry the startup token cookie. Nothing is written and no
//! process is attached.

use std::net::{SocketAddr, TcpStream};
use std::path::Path;

use crate::commands::status::web::access::{AccessPolicy, OriginRequirement};
use crate::commands::status::web::connection::respond;
use crate::commands::status::web::http::RequestHead;
use crate::commands::status::web::TerminalLane;
use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::tmux::read_history;
use crate::validation::validate_id;

/// Path prefix of the history route.
pub(in crate::commands::status::web) const ROUTE_PREFIX: &str = "/api/terminal-history/";

/// The status, reason phrase, content type and body of one response.
type Reply = (u16, &'static str, &'static str, Vec<u8>);

const TEXT: &str = "text/plain; charset=utf-8";

/// Answer a history request, or the refusal for one that has not passed the
/// terminal lane's gate.
pub(in crate::commands::status::web) fn serve(
    stream: &mut TcpStream,
    head: &RequestHead,
    base: &Path,
    lane: Option<&TerminalLane>,
    policy: &AccessPolicy,
    local: SocketAddr,
) {
    let (status, reason, content_type, body) = match lane {
        None => refusal(404, "Not Found", "not found"),
        Some(_) if !policy.origin_allowed(local, head, OriginRequirement::Optional) => {
            refusal(403, "Forbidden", "origin not allowed")
        }
        Some(lane) if !lane.cookie_matches(head.cookie.as_deref()) => {
            refusal(401, "Unauthorized", "dashboard token required")
        }
        Some(_) => history_reply(base, &head.path),
    };
    respond(stream, head, status, reason, content_type, &body);
}

fn refusal(status: u16, reason: &'static str, text: &'static str) -> Reply {
    (status, reason, TEXT, text.as_bytes().to_vec())
}

/// The history file of the session `path` names. The id is validated before it
/// is joined into a path, and the read is bounded and refuses a symlink. A
/// failure is logged and answered as absent, so no response names a path.
fn history_reply(base: &Path, path: &str) -> Reply {
    let session_id = path
        .strip_prefix(ROUTE_PREFIX)
        .filter(|id| validate_id(id).is_ok());
    let Some(session_id) = session_id else {
        return refusal(404, "Not Found", "not found");
    };
    let history = WorkDir::new(base).and_then(|work_dir| read_history(work_dir.root(), session_id));
    match history {
        Ok(Some(bytes)) => (200, "OK", "application/octet-stream", bytes),
        Ok(None) => refusal(404, "Not Found", "no terminal history for that session"),
        Err(error) => {
            tracing::warn!("dashboard could not read terminal history: {error:#}");
            refusal(404, "Not Found", "no terminal history for that session")
        }
    }
}

#[cfg(test)]
#[path = "tests_history.rs"]
mod tests;
