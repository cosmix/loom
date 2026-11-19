//! The one-line terminal notices for chat sessions starting and ending, and the name and socket they print.

use crate::plan_server::lifecycle;
use crate::plan_session::bridge::registry::read_record;
use crate::plan_session::bridge::{Bridge, EndOutcome, TEARDOWN_FAILED};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::planning::runtime::RuntimePaths;

/// The session's display name and tmux socket: the chat's view, else its record.
pub(super) fn describe(
    bridge: &Bridge,
    runtime: &RuntimePaths,
    id: &str,
    kind: &SessionKind,
) -> (String, String) {
    let snapshot = bridge.chat_snapshot(false);
    let previous = snapshot.previous.map(|ended| ended.session);
    if let Some(view) = snapshot
        .session
        .into_iter()
        .chain(previous)
        .find(|v| v.id == id)
    {
        return (view.name, view.socket);
    }
    let record = read_record(&SessionDir::new(runtime.sessions.join(id)));
    let socket = record.map(|record| record.socket).unwrap_or_default();
    (kind.label().to_owned(), socket)
}

pub(super) fn started_line(kind: &SessionKind, name: &str, socket: &str) -> String {
    format!(
        "session started: {} \"{}\" (watch: tmux -L {} attach -r)",
        lifecycle::safe(kind.label()),
        lifecycle::safe(name),
        lifecycle::safe(socket)
    )
}

/// A failed teardown keeps the session and retries it, so it gets its own line.
pub(super) fn ended_line(
    id: &str,
    kind: &SessionKind,
    name: &str,
    socket: &str,
    outcome: &EndOutcome,
) -> String {
    let outcome = match outcome {
        EndOutcome::Failed(why) if why.starts_with(TEARDOWN_FAILED) => {
            let (id, socket) = (lifecycle::safe(id), lifecycle::safe(socket));
            return format!("{TEARDOWN_FAILED}: session {id}, socket {socket}");
        }
        EndOutcome::Failed(why) => format!("failed: {why}"),
        EndOutcome::Closed => "closed".to_owned(),
        EndOutcome::Marker => "marker".to_owned(),
        EndOutcome::TornDown => "torn down".to_owned(),
    };
    format!(
        "session ended: {} \"{}\" ({})",
        lifecycle::safe(kind.label()),
        lifecycle::safe(name),
        lifecycle::safe(&outcome)
    )
}

#[cfg(test)]
mod line_tests {
    use super::{ended_line, started_line};
    use crate::plan_session::bridge::EndOutcome;
    use crate::plan_session::kind::SessionKind;

    #[test]
    fn the_terminal_lines_name_the_socket_and_neutralise_escapes() {
        let (kind, socket) = (SessionKind::Authoring, "loom-plan-3f9a");
        let started = started_line(&kind, "chat \x1b]0;owned\x07", socket);
        assert!(started.starts_with("session started: authoring \"chat "));
        assert!(started.ends_with("(watch: tmux -L loom-plan-3f9a attach -r)"));
        assert!(!started.contains(['\x1b', '\x07']), "{started:?}");
        let failed = EndOutcome::Failed("teardown failed: kill-server failed".into());
        let line = ended_line("id-1", &kind, "chat", socket, &failed);
        assert_eq!(line, "teardown failed: session id-1, socket loom-plan-3f9a");
        let closed = ended_line("id", &SessionKind::Summary, "s", "x", &EndOutcome::Closed);
        assert_eq!(closed, "session ended: summary \"s\" (closed)");
    }
}
