//! The bridge's start path: `start_session` reserves a session and runs its guidance, then the
//! driver's launch writes the provisional record, spawns and completes the record.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use chrono::Utc;

use super::commands::{Out, Step};
use super::host::{socket_name, LaunchSpec, TeardownFailed};
use super::registry::{remove_session_dir, write_record, SessionRecord, SCHEMA};
use super::session::{stamp, Opening, Phase, Recovery, Session};
use super::{Bridge, BridgeConfig, BridgeError, BridgeEvent, EndOutcome, GuidanceFn};
use super::{LaunchGuidance, StartRequest};
use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_create_new_dir_in_workdir, safe_open_dirfd,
};
use crate::plan_session::input::{check_delivered, sanitize};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{ChatDelta, Failure, FailureKind, Origin};
use crate::plan_session::state::Input;

/// The start request's parts the session keeps.
struct Start {
    origin: Option<Origin>,
    offer: Option<String>,
    extra_env: Vec<(String, String)>,
}

/// A session the driver claimed for launching.
struct Launch {
    id: String,
    spec: Box<LaunchSpec>,
    record: SessionRecord,
    opening: Option<String>,
}

impl Bridge {
    /// Reserve the session, create its directory, run the guidance; the driver launches it.
    pub fn start_session(&self, req: StartRequest) -> Result<String, BridgeError> {
        if self.config.home.as_os_str().is_empty() {
            return Err(BridgeError::Refused("HOME is not set".to_owned()));
        }
        let (id, dir, leftovers) = self.reserve(&req.kind, req.feature, req.plan)?;
        for leftover in leftovers {
            if let Err(error) = remove_session_dir(&leftover) {
                tracing::warn!(%error, "cannot remove an ended session's directory");
            }
        }
        let start = Start {
            origin: req.opening_origin,
            offer: req.offer,
            extra_env: req.extra_env,
        };
        match self.prepare(&dir, req.guidance) {
            Ok(guidance) => {
                self.ready(&id, guidance, start);
                self.flush();
                Ok(id)
            }
            Err(error) => {
                self.lock().sessions.remove(&id);
                if let Err(cleanup) = remove_session_dir(&dir) {
                    tracing::warn!(error = %cleanup, "cannot remove a session directory");
                }
                Err(error)
            }
        }
    }

    /// Check the one-chat rules and insert the session as Preparing; hand back the
    /// directories of failed sessions, which a new start removes.
    fn reserve(
        &self,
        kind: &SessionKind,
        feature: Option<String>,
        plan: Option<String>,
    ) -> Result<(String, SessionDir, Vec<SessionDir>), BridgeError> {
        let mut inner = self.lock();
        if !matches!(inner.recovery, Recovery::Done(_)) {
            return Err(BridgeError::Busy("recovery in progress"));
        }
        if inner.stopping {
            return Err(BridgeError::Busy("the server is stopping"));
        }
        let chat = |kind: &SessionKind| !matches!(kind, SessionKind::Summary);
        let mut running = inner.sessions.values().filter(|session| session.active());
        if running.any(|session| chat(&session.kind) == chat(kind)) {
            return Err(BridgeError::Busy(match chat(kind) {
                true => "a chat is running",
                false => "a summary session is running",
            }));
        }
        let failed = inner
            .sessions
            .values()
            .filter(|s| matches!(s.phase, Phase::Leftover));
        let ids: Vec<String> = failed.map(|session| session.id.clone()).collect();
        let removed = ids.iter().filter_map(|id| inner.sessions.remove(id));
        let leftovers = removed.map(|session| session.dir).collect();
        let id = uuid::Uuid::new_v4().to_string();
        let dir = SessionDir::new(self.config.runtime.sessions.join(&id));
        let record = SessionRecord {
            schema: SCHEMA,
            id: id.clone(),
            kind: kind.clone(),
            socket: socket_name(&id),
            tmux_session: format!("plan-{}", kind.label()),
            pid: None,
            start_time: None,
            transcript_path: None,
            started_at: stamp(Utc::now()),
            feature,
            plan,
            children: Vec::new(),
        };
        let session = Session::new(record, dir.clone(), self.config.project_root.clone());
        inner.sessions.insert(id.clone(), session);
        Ok((id, dir, leftovers))
    }

    fn prepare(
        &self,
        dir: &SessionDir,
        guidance: GuidanceFn,
    ) -> Result<LaunchGuidance, BridgeError> {
        create_session_dir(&self.config.runtime.dir, dir)
            .map_err(|error| BridgeError::Io(format!("{error:#}")))?;
        let guidance = guidance(dir).map_err(|error| BridgeError::Refused(format!("{error:#}")))?;
        if guidance.opening_on_argv && !is_command_opening(&guidance.opening_message) {
            let why = "an opening on the launch line must be a command and a plan path";
            return Err(BridgeError::Refused(why.to_owned()));
        }
        Ok(guidance)
    }

    /// The session waits for the driver's launch; a chat session replaces the chat.
    fn ready(&self, id: &str, guidance: LaunchGuidance, start: Start) {
        let mut inner = self.lock();
        let kind = inner.sessions.get(id).map(|session| session.kind.clone());
        let chat = kind.is_some_and(|kind| !matches!(kind, SessionKind::Summary));
        let mut step = Step::new(chat);
        if chat {
            inner.previous = None;
            inner.chat = Some(id.to_owned());
            step.out.push(Out::Delta(Box::new(ChatDelta::Cleared)));
        }
        if let Some(session) = inner.sessions.get_mut(id) {
            session.configure(guidance, start, &self.config);
            session.sync(&mut step);
        }
        inner.outbox.append(&mut step.out);
    }

    /// Launch every session waiting for it, one after another (a launch blocks the tick).
    pub(super) fn spawn_pending(&self) {
        let claims: Vec<Launch> = {
            let mut inner = self.lock();
            if inner.stopping {
                return;
            }
            let sessions = inner.sessions.values_mut();
            sessions.filter_map(Session::claim_launch).collect()
        };
        for claim in claims {
            self.launch(claim);
        }
    }

    /// Provisional record, spawn, complete record (px-state-11); only then `Started`.
    fn launch(&self, claim: Launch) {
        let Launch {
            id,
            spec,
            mut record,
            opening,
        } = claim;
        if let Some(Err(refusal)) = opening.as_deref().map(check_delivered) {
            return self.fail_start(
                &id,
                format!("the opening message cannot be sent: {refusal}"),
            );
        }
        if let Err(error) = write_record(&spec.dir, &record) {
            return self.fail_start(&id, format!("{error:#}"));
        }
        let spawned = match self.host.spawn(&spec) {
            Ok(spawned) => spawned,
            Err(error) => return self.spawn_failed(&id, record, error),
        };
        record.pid = Some(spawned.identity.pid);
        record.start_time = spawned.identity.start_time;
        if let Err(error) = write_record(&spec.dir, &record) {
            return match self.host.teardown(&spec.socket) {
                Ok(()) => self.fail_start(&id, format!("{error:#}")),
                Err(teardown) => self.unrecorded_teardown_failed(&id, record, &error, &teardown),
            };
        }
        let next_identity = Instant::now() + self.config.identity_interval;
        self.step(&id, |session, _, step| {
            // Field by field: a kickoff's confirmation may have named the owner meanwhile.
            session.record.pid = record.pid;
            session.record.start_time = record.start_time;
            session.phase = Phase::Live;
            session.next_identity = next_identity;
            let (id, kind) = (session.id.clone(), session.kind.clone());
            step.event(BridgeEvent::Started { id, kind });
        });
    }

    /// A failed spawn fails the session; one whose teardown failed too may have left its tmux
    /// server running, so its session keeps the provisional record (common.md: a destructive
    /// step on unverifiable evidence does nothing).
    fn spawn_failed(&self, id: &str, record: SessionRecord, error: anyhow::Error) {
        match error.downcast::<TeardownFailed>() {
            Ok(failed) => {
                self.unrecorded_teardown_failed(id, record, &failed.error, &failed.teardown);
            }
            Err(error) => self.fail_start(id, format!("{error:#}")),
        }
    }

    pub(super) fn fail_start(&self, id: &str, message: String) {
        tracing::warn!(session = %id, %message, "a planning session did not start");
        let failure = Failure {
            kind: FailureKind::Spawn,
            message: message.clone(),
        };
        self.step(id, |session, _, _| {
            session.machine.apply(Input::SpawnFailed);
            session.failure = Some(failure);
        });
        self.conclude(id, EndOutcome::Failed(message), Some(Phase::Leftover));
    }

    /// The spawn may have left its server running, the record does not name its process and
    /// the teardown failed: the session keeps its directory (the provisional record names the
    /// socket) and the record it has, and the teardown is retried. A `Leftover` would let the
    /// next start delete the only handle.
    fn unrecorded_teardown_failed(
        &self,
        id: &str,
        record: SessionRecord,
        error: &anyhow::Error,
        teardown: &anyhow::Error,
    ) {
        self.with_session(id, |session, _| {
            session.machine.apply(Input::SpawnFailed);
            session.record = record;
        });
        let detail = format!("{teardown:#} (after: {error:#})");
        self.teardown_failed(id, FailureKind::Spawn, detail);
    }
}

impl Session {
    fn configure(&mut self, mut guidance: LaunchGuidance, start: Start, config: &BridgeConfig) {
        // The opening reaches the launch line only as a validated command (px-security-04,
        // px-seams-R04); otherwise the driver pastes it after the first SessionStart.
        let opening = std::mem::take(&mut guidance.opening_message);
        let argv = guidance.opening_on_argv.then(|| opening.clone());
        self.name = guidance.display_name.clone();
        self.model = guidance.model.clone();
        self.offer = start.offer;
        self.mapper.expect_opening(start.origin);
        if argv.is_none() {
            let text = sanitize(&opening);
            self.flow.opening = Some(Opening {
                text,
                origin: start.origin,
            });
        }
        let spec = LaunchSpec {
            id: self.id.clone(),
            kind: self.kind.clone(),
            dir: self.dir.clone(),
            socket: self.record.socket.clone(),
            tmux_session: self.record.tmux_session.clone(),
            guidance,
            extra_env: start.extra_env,
            opening_argv: argv,
            project_root: config.project_root.clone(),
            runtime: config.runtime.clone(),
            loom_bin: config.loom_bin.clone(),
            claude_bin: config.claude_bin.clone(),
            home: config.home.clone(),
        };
        self.phase = Phase::Spawning(Box::new(spec));
    }

    fn claim_launch(&mut self) -> Option<Launch> {
        if self.flow.end.is_some() || !matches!(self.phase, Phase::Spawning(_)) {
            return None;
        }
        match std::mem::replace(&mut self.phase, Phase::Launching) {
            Phase::Spawning(spec) => Some(Launch {
                id: self.id.clone(),
                spec,
                record: self.record.clone(),
                opening: self
                    .flow
                    .opening
                    .as_ref()
                    .map(|opening| opening.text.clone()),
            }),
            other => {
                self.phase = other;
                None
            }
        }
    }
}

/// `sessions/` (if missing) and `sessions/<id>/answers/`, 0700, through `safe_fs`.
fn create_session_dir(runtime: &Path, dir: &SessionDir) -> anyhow::Result<()> {
    let fd = safe_open_dirfd(runtime)?;
    let name = dir
        .path()
        .file_name()
        .context("a session directory has a name")?;
    let sessions = Path::new("sessions");
    let relative = sessions.join(name);
    safe_create_dir_all_in_workdir(fd.as_raw_fd(), sessions, 0o700)?;
    safe_create_new_dir_in_workdir(fd.as_raw_fd(), &relative, 0o700)?;
    safe_create_new_dir_in_workdir(fd.as_raw_fd(), &relative.join("answers"), 0o700)
}

/// `^/[a-z0-9:-]+ doc/plans/[^\s]+\.md$`: a command and a plan path, no document content.
fn is_command_opening(text: &str) -> bool {
    let Some((command, path)) = text.split_once(' ') else {
        return false;
    };
    let name = command.strip_prefix('/').unwrap_or("");
    let plan = path.strip_prefix("doc/plans/").unwrap_or("");
    let command_char =
        |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ':' || c == '-';
    !name.is_empty()
        && name.chars().all(command_char)
        && plan.len() > ".md".len()
        && plan.ends_with(".md")
        && !plan.chars().any(char::is_whitespace)
}
