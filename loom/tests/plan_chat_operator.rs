//! Operator proof of a real attached chat turn and restoration through the public API.

#[path = "plan_chat_operator/support.rs"]
mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use loom::plan_server::files::texts::{TextStore, TurnId};
use loom::plan_server::ids::PublicId;
use loom::plan_server::wire::chat::{ChatCommand, ChatFrame};
use loom::plan_server::wire::message::PlanMessage;
use loom::plan_server::wire::review::{RestoreFileResult, RestoreRequest, RestoreResponse};
use loom::plan_server::RunningServer;
use loom::plan_session::bridge::Bridge;
use loom::plan_session::kind::SessionDir;
use loom::plan_session::model::{ChatSnapshot, Item, PermissionStatus, SessionState, Turn};
use loom::plan_session::spool::SpoolReader;
use loom::planning::runtime::create_runtime_dir;
use reqwest::blocking::Client;
use support::{
    attachment, bootstrap, claude_version, connect, fixture, hello, http_client, next,
    prerequisites, send, LoomHome, ServerGuard, Socket, SOURCE, TARGET,
};

#[test]
#[ignore = "needs tmux and a logged-in claude; run by the operator in integration-verify"]
#[serial_test::serial]
fn real_session_attach_send_restore() {
    let version = claude_version();
    println!("claude --version: {version}");
    match std::panic::catch_unwind(operator) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("claude --version: {version}\n{error:#}"),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic in operator harness");
            panic!("claude --version: {version}\n{message}");
        }
    }
}

fn operator() -> Result<()> {
    prerequisites()?;
    let repo = fixture()?;
    let _home = LoomHome::new()?;
    let guard = ServerGuard::start(repo.path())?;
    let server = guard.0.as_ref().context("server guard is empty")?;
    create_runtime_dir(server.ctx.runtime())?;
    let bridge = server
        .ctx
        .extensions()
        .get::<Bridge>()
        .context("bridge not installed")?;
    let (tx, rx) = mpsc::channel();
    bridge.on_recovered(Box::new(move |_| {
        let _ = tx.send(());
    }));
    rx.recv_timeout(Duration::from_secs(30))
        .context("bridge recovery did not finish")?;
    let client = http_client()?;
    let cookies = bootstrap(server, &client)?;
    let mut socket = connect(server, &cookies)?;
    let you = hello(&mut socket)?;
    send(
        &mut socket,
        ChatCommand::Start {
            client_id: Some("operator-start".into()),
            offer: None,
            feature: Some("demo".into()),
            text: Some(format!(
                "Read the attached source. Immediately append the exact sentence \
            'Operator attachment verified.' as a new paragraph to {TARGET}, preserving its \
            existing text. Edit only that file, do not run git, ask no questions, then stop."
            )),
            attachments: vec![attachment()],
        },
    )?;
    let (session, turn) = wait_turn(server, &bridge, &mut socket, &you)?;
    restore(server, &client, &cookies, &session, &turn)?;
    socket.close(None)?;
    Ok(())
}

/// What the WebSocket has told the test so far.
#[derive(Default)]
struct Seen {
    holder: bool,
    accepted: bool,
    approved: BTreeSet<String>,
}

impl Seen {
    fn observe(&mut self, message: Option<PlanMessage>, you: &PublicId) -> Result<()> {
        match message {
            Some(PlanMessage::Authoring(view)) => {
                self.holder = view.holder.is_some_and(|holder| holder.public_id == *you);
            }
            Some(PlanMessage::Chat(frame)) => {
                if let ChatFrame::Result(result) = *frame {
                    ensure!(result.ok, "chat command refused: {:?}", result.error);
                    self.accepted |= result.client_id.as_deref() == Some("operator-start");
                }
            }
            Some(PlanMessage::Refused(refusal)) => bail!("WebSocket refused: {refusal:?}"),
            _ => {}
        }
        Ok(())
    }
}

fn wait_turn(
    server: &RunningServer,
    bridge: &Bridge,
    socket: &mut Socket,
    you: &PublicId,
) -> Result<(String, Turn)> {
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut seen = Seen::default();
    loop {
        seen.observe(next(socket)?, you)?;
        let snapshot = bridge.chat_snapshot(true);
        if let Some(done) = completed_turn(server, bridge, socket, &snapshot, &mut seen)? {
            return Ok(done);
        }
        ensure!(
            Instant::now() < deadline,
            "no completed changed-files turn with main Stop within 10 minutes; \
             accepted={}, holder={}, session={:?}, turns={:?}; \
             in-process hooks currently use current_exe(), the test binary",
            seen.accepted,
            seen.holder,
            snapshot.session,
            snapshot.turns
        );
    }
}

/// Answers pending permissions and returns the first finished turn that changed files.
fn completed_turn(
    server: &RunningServer,
    bridge: &Bridge,
    socket: &mut Socket,
    snapshot: &ChatSnapshot,
    seen: &mut Seen,
) -> Result<Option<(String, Turn)>> {
    let Some(session) = &snapshot.session else {
        return Ok(None);
    };
    ensure!(
        session.state != SessionState::Failed,
        "chat failed: {:?}",
        session.failure
    );
    for turn in &snapshot.turns {
        approve_permissions(socket, turn, &mut seen.approved)?;
        if seen.accepted
            && seen.holder
            && !turn.open
            && !turn.changed_files.is_empty()
            && session.state == SessionState::Idle
            && !bridge.turn_running()
            && has_stop(server, &session.id, turn)?
        {
            return Ok(Some((session.id.clone(), turn.clone())));
        }
    }
    Ok(None)
}

fn approve_permissions(
    socket: &mut Socket,
    turn: &Turn,
    approved: &mut BTreeSet<String>,
) -> Result<()> {
    for item in &turn.items {
        if let Item::Permission {
            request_id,
            status: PermissionStatus::Pending,
            ..
        } = item
        {
            if approved.insert(request_id.clone()) {
                send(
                    socket,
                    ChatCommand::Permission {
                        client_id: Some(format!("operator-permit-{request_id}")),
                        request_id: request_id.clone(),
                        allow: true,
                        note: None,
                    },
                )?;
            }
        }
    }
    Ok(())
}

fn has_stop(server: &RunningServer, session: &str, turn: &Turn) -> Result<bool> {
    let started = chrono::DateTime::parse_from_rfc3339(&turn.started_at)?;
    let dir = SessionDir::new(server.ctx.runtime().sessions.join(session));
    let mut spool = SpoolReader::new(dir.events());
    let mut stopped = false;
    for event in spool.read_new() {
        if event.session_id.as_deref() != Some(session)
            || event.received_at < started
            || event
                .payload
                .get("agent_id")
                .is_some_and(|id| !id.is_null())
        {
            continue;
        }
        ensure!(
            event.event != "StopFailure",
            "main StopFailure: {:?}",
            event.payload
        );
        stopped |= event.event == "Stop";
    }
    Ok(stopped)
}

/// The turn-start text of the changed target file, checked against the fixture.
fn turn_start_text(server: &RunningServer, session: &str, turn: &Turn) -> Result<String> {
    let changed = turn
        .changed_files
        .iter()
        .find(|file| file.path == TARGET)
        .context("the turn did not change the requested source file")?;
    let store = server
        .ctx
        .extensions()
        .get::<TextStore>()
        .context("text store not installed")?;
    let pin = store
        .turn_pin(
            &TurnId(format!("{session}/{}", turn.id)),
            Path::new(&changed.path),
        )
        .context("changed file has no turn-start text")?;
    ensure!(
        pin.text.as_ref() == SOURCE,
        "turn-start text differs from the fixture: {:?}",
        pin.text
    );
    let text: &str = pin.text.as_ref();
    Ok(text.to_owned())
}

/// POSTs `/api/restore` for `path` and checks the single restored file in the response.
fn post_restore(
    server: &RunningServer,
    client: &Client,
    cookies: &str,
    session: &str,
    turn: &Turn,
    path: &str,
) -> Result<()> {
    let response = client
        .post(format!("http://{}/api/restore", server.local))
        .header("Origin", format!("http://{}", server.local))
        .header("Cookie", cookies)
        .json(&RestoreRequest {
            session: session.into(),
            turn: turn.id.clone(),
            paths: vec![path.to_owned()],
            force: false,
        })
        .send()?;
    let status = response.status();
    let body = response.text()?;
    ensure!(status.is_success(), "POST /api/restore: {status}: {body}");
    let restored: RestoreResponse = serde_json::from_str(&body)?;
    ensure!(
        restored.files.len() == 1
            && restored.files[0].path == path
            && matches!(restored.files[0].result, RestoreFileResult::Restored),
        "restore response: {body}"
    );
    Ok(())
}

fn restore(
    server: &RunningServer,
    client: &Client,
    cookies: &str,
    session: &str,
    turn: &Turn,
) -> Result<()> {
    let start_text = turn_start_text(server, session, turn)?;
    let path = server.ctx.root().join(TARGET);
    ensure!(
        fs::read_to_string(&path)? != start_text,
        "the turn made no observable edit"
    );
    post_restore(server, client, cookies, session, turn, TARGET)?;
    let actual = fs::read_to_string(&path)?;
    ensure!(
        actual == start_text,
        "restored file differs from turn-start text; \
        expected {:?}, got {actual:?}",
        start_text
    );
    Ok(())
}
