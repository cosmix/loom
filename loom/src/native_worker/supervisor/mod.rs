//! Supervisor process for one native worker: boundary, protocol loop, teardown and settlement.
pub mod control;
mod io;
mod preflight;
mod teardown;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

pub use io::SystemClock;
pub use teardown::settle;

use self::control::{ControlEvent, ControlPoller};
use self::io::start_pipes;
use self::teardown::{settle_exit, settle_unspawned};
use super::boundary::{self, BoundarySpec, Quiescence, SpawnedBoundary};
use super::machine::{Action, Input, Machine, MachineConfig};
use super::model::{
    BoundaryIdentity, NativeEvent, NativeManifest, Outcome, SandboxMode, INTERRUPT_GRACE_SECS,
    MAX_BRIEF_BYTES, PROGRESS_EVENT_INTERVAL_SECS,
};
use super::protocol::frame::{classify, FrameError, Transport};
use super::store::{self, EventLog};
use super::{prompt, proxy};
use crate::fs::safe_fs::{safe_create_new_in_workdir, safe_open_dirfd};

/// Time source of the loop; tests step it.
pub trait Clock {
    fn now(&self) -> Instant;
}

const CONTROL_POLL: Duration = Duration::from_millis(100);
const RECV_TIMEOUT: Duration = Duration::from_millis(200);
const GRACE: Duration = Duration::from_secs(INTERRUPT_GRACE_SECS);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(PROGRESS_EVENT_INTERVAL_SECS);

/// Drives the reducer over `transport` until it tears down; returns the
/// teardown's outcome and reason. Any record failure is an `Err`.
pub fn run_session<T: Transport>(
    dir: &Path,
    transport: &mut T,
    clock: &dyn Clock,
) -> Result<(Outcome, String)> {
    Session::new(dir, transport, clock)?.run()
}

struct Session<'a, T: Transport> {
    dir: &'a Path,
    transport: &'a mut T,
    clock: &'a dyn Clock,
    machine: Machine,
    log: EventLog,
    control: ControlPoller,
    deadline: Duration,
    deadline_at: Option<Instant>,
    grace_at: Option<Instant>,
    last_poll: Option<Instant>,
    last_event: Instant,
    teardown: Option<(Outcome, String)>,
}

impl<'a, T: Transport> Session<'a, T> {
    fn new(dir: &'a Path, transport: &'a mut T, clock: &'a dyn Clock) -> Result<Self> {
        let manifest = store::read_manifest(dir).context("manifest.json")?;
        let brief = super::store_io::read_regular(dir, "brief.md", MAX_BRIEF_BYTES)
            .context("brief.md")?
            .ok_or_else(|| anyhow!("brief.md is missing"))?;
        let brief = String::from_utf8(brief).context("brief.md is not UTF-8")?;
        let events = store::read_events(dir).context("events.jsonl")?;
        let sandbox_mode = events
            .iter()
            .rev()
            .find_map(|record| match record.event {
                NativeEvent::BoundaryCreated { sandbox_mode } => Some(sandbox_mode),
                _ => None,
            })
            .unwrap_or(SandboxMode::WorkspaceWrite);
        let machine = Machine::new(MachineConfig {
            cwd: manifest.workspace_root.clone(),
            model: manifest.model.clone(),
            effort: manifest.effort.clone(),
            task_text: prompt::task_text(&manifest.brief_path, &brief),
            sandbox_mode,
            client_version: env!("CARGO_PKG_VERSION").into(),
        });
        Ok(Self {
            dir,
            transport,
            clock,
            machine,
            log: EventLog::open(dir).context("events.jsonl")?,
            control: ControlPoller::new(dir),
            deadline: Duration::from_secs(manifest.deadline_secs),
            deadline_at: None,
            grace_at: None,
            last_poll: None,
            last_event: clock.now(),
            teardown: None,
        })
    }

    fn run(mut self) -> Result<(Outcome, String)> {
        let start = self.last_event;
        self.feed(Input::Start, start)?;
        while !self.machine.is_finished() {
            self.iteration()?;
        }
        self.teardown
            .ok_or_else(|| anyhow!("the reducer finished without a teardown"))
    }

    /// One pass; reads the clock exactly once and uses that reading throughout.
    fn iteration(&mut self) -> Result<()> {
        let now = self.clock.now();
        if self.last_poll.is_none_or(|at| now >= at + CONTROL_POLL) {
            self.last_poll = Some(now);
            self.poll_control(now)?;
        }
        let arrived = self.receive(now)?;
        if self.deadline_at.is_some_and(|at| now >= at) {
            self.deadline_at = None;
            self.feed(Input::Deadline, now)?;
        }
        if self.grace_at.is_some_and(|at| now >= at) {
            self.grace_at = None;
            self.feed(Input::GraceElapsed, now)?;
        }
        let quiet = now.saturating_duration_since(self.last_event);
        if arrived && !self.machine.is_finished() && quiet >= PROGRESS_INTERVAL {
            self.persist(NativeEvent::Progress, now)?;
        }
        Ok(())
    }

    fn poll_control(&mut self, now: Instant) -> Result<()> {
        let (events, error) = self.control.poll();
        if let Some(error) = error {
            eprintln!("control: {error}");
        }
        for event in events {
            let input = match event {
                ControlEvent::Cancel(cancel) => Input::Cancel {
                    reason: cancel.reason,
                },
                ControlEvent::Message(message) => Input::Control(message),
            };
            self.feed(input, now)?;
        }
        Ok(())
    }

    /// Feeds at most one provider frame; true when a frame arrived.
    fn receive(&mut self, now: Instant) -> Result<bool> {
        match self.transport.recv(RECV_TIMEOUT) {
            Ok(Some(frame)) => {
                if let Ok(incoming) = classify(frame) {
                    self.feed(Input::Frame(incoming), now)?;
                }
                Ok(true)
            }
            Ok(None) | Err(FrameError::Oversized | FrameError::Malformed(_)) => Ok(false),
            Err(FrameError::Eof | FrameError::Io(_)) => {
                self.feed(Input::TransportClosed, now)?;
                Ok(false)
            }
        }
    }

    fn feed(&mut self, input: Input, now: Instant) -> Result<()> {
        let mut inputs = VecDeque::from([input]);
        while let Some(input) = inputs.pop_front() {
            for action in self.machine.step(input) {
                self.apply(action, now, &mut inputs)?;
            }
        }
        Ok(())
    }

    fn apply(&mut self, action: Action, now: Instant, inputs: &mut VecDeque<Input>) -> Result<()> {
        match action {
            Action::Send(frame) => {
                if let Err(error) = self.transport.send(&frame) {
                    eprintln!("send to codex app-server failed: {error}");
                    inputs.push_back(Input::TransportClosed);
                }
            }
            Action::Persist(event) => self.persist(event, now)?,
            Action::StoreReport { text, complete } => {
                let info = store::write_report(self.dir, &text).context("report.md")?;
                let (bytes, sha256) = (info.bytes, info.sha256);
                let event = NativeEvent::ReportStored {
                    bytes,
                    sha256,
                    complete,
                };
                self.persist(event, now)?;
            }
            Action::ArmDeadline => self.deadline_at = Some(now + self.deadline),
            Action::ArmGrace => self.grace_at = Some(now + GRACE),
            Action::Teardown { outcome, reason } => self.teardown = Some((outcome, reason)),
        }
        Ok(())
    }

    fn persist(&mut self, event: NativeEvent, now: Instant) -> Result<()> {
        self.log.append(event).context("events.jsonl append")?;
        self.last_event = now;
        Ok(())
    }
}

/// Runs the invocation in `dir` from admission to settlement.
///
/// `Ok(SUCCESS)` once settled (or when the record was already terminal),
/// `Ok(1)` when settling could not append, `Err` for a refusal.
pub fn run_from_dir(dir: &Path) -> Result<ExitCode> {
    claim(dir)?;
    let Some(manifest) = admitted_manifest(dir)? else {
        return Ok(ExitCode::SUCCESS);
    };
    let (codex_home, codex_program, loom_program) = match preflight::preflight(&manifest) {
        Ok(tools) => tools,
        Err(reason) => return Ok(settle_unspawned(dir, &reason)),
    };
    let socket_dir = match tempfile::Builder::new().prefix("loom-nw-").tempdir() {
        Ok(socket_dir) => socket_dir,
        Err(e) => return Ok(settle_unspawned(dir, &format!("socket directory: {e}"))),
    };
    let proxy_socket = socket_dir.path().join("proxy.sock");
    let proxy = match proxy::start(&proxy_socket, &crate::codex::CODEX_SANDBOX_DOMAINS) {
        Ok(proxy) => proxy,
        Err(e) => return Ok(settle_unspawned(dir, &format!("proxy start failed: {e:#}"))),
    };
    let mode = if boundary::nested_sandbox_supported() {
        SandboxMode::WorkspaceWrite
    } else {
        SandboxMode::External
    };
    let spec = BoundarySpec {
        worktree: manifest.workspace_root,
        codex_home,
        codex_program,
        proxy_socket,
        loom_program,
    };
    // From this (main) thread, before any reader, writer or drain thread exists.
    let code = match boundary::spawn(&spec) {
        Ok(spawned) => supervise_boundary(dir, spawned, mode),
        Err(e) => {
            teardown::settle_spawn_failure(dir, &format!("{e:#}"), boundary::spawn_started(&e))
        }
    };
    drop(proxy);
    drop(socket_dir);
    Ok(code)
}

/// Creates `supervisor.json` exclusively: whoever creates it owns the
/// invocation, so a second supervisor (or a daemon tombstone) refuses here.
fn claim(dir: &Path) -> Result<()> {
    let pid = std::process::id();
    let start = crate::process::process_start_time(pid)
        .context("cannot read the supervisor's process start time")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "pid": pid as i32,
        "start": start.to_string(),
    }))?;
    let fd = safe_open_dirfd(dir)?;
    safe_create_new_in_workdir(fd.as_raw_fd(), Path::new("supervisor.json"), &body)
        .context("supervisor.json already exists or cannot be created")
}

/// The manifest of a record that is exactly `[Admitted]` with no boundary;
/// `None` when it already holds a `Terminal` event.
fn admitted_manifest(dir: &Path) -> Result<Option<NativeManifest>> {
    let manifest = store::read_manifest(dir).context("manifest.json")?;
    let events = store::read_events(dir).context("events.jsonl")?;
    if events
        .iter()
        .any(|record| matches!(record.event, NativeEvent::Terminal { .. }))
    {
        return Ok(None);
    }
    if events.len() != 1 || events[0].event != NativeEvent::Admitted {
        bail!("invocation is not in the admitted state");
    }
    if store::read_boundary(dir)
        .context("boundary.json")?
        .is_some()
    {
        bail!("boundary.json already exists");
    }
    Ok(Some(manifest))
}

/// Records the boundary, runs the session, tears the boundary down and settles.
/// Only the pipes move out of `spawned`; the rest (the process and the mounted
/// Cargo home) lives until this function returns, after the boundary is retired.
fn supervise_boundary(dir: &Path, mut spawned: SpawnedBoundary, mode: SandboxMode) -> ExitCode {
    use Outcome::Unknown;
    let identity = match boundary::identity_for(&spawned) {
        Ok(identity) => identity,
        Err(e) => {
            // Closing the input first lets the app-server exit on EOF instead
            // of `stop_process` waiting out the whole grace.
            drop(spawned.stdin);
            teardown::stop_process(&mut spawned.process);
            let unchecked = format!("no boundary identity, namespace unchecked: {e:#}");
            let reason = format!("could not record boundary: {e:#}");
            return settle_exit(dir, Unknown, &reason, Quiescence::Unverified(unchecked));
        }
    };
    let (stdin, stdout, stderr) = (spawned.stdin, spawned.stdout, spawned.stderr);
    let (result, threads) = match record_boundary(dir, &identity, mode) {
        Ok(()) => match start_pipes(dir, stdin, stdout, stderr) {
            Ok(mut transport) => {
                let result = run_session(dir, &mut transport, &SystemClock);
                (result, Some(transport.close_input()))
            }
            Err(e) => (Err(anyhow!(e).context("start the pipe threads")), None),
        },
        Err(e) => {
            drop((stdin, stdout, stderr));
            (
                Ok((Unknown, format!("could not record boundary: {e:#}"))),
                None,
            )
        }
    };
    teardown::stop_process(&mut spawned.process);
    let quiescence = boundary::verify_quiescent(&identity);
    if let (Quiescence::Released, Some(threads)) = (&quiescence, threads) {
        threads.join();
    }
    let (outcome, reason) =
        result.unwrap_or_else(|e| (Unknown, format!("event log or record failure: {e:#}")));
    settle_exit(dir, outcome, &reason, quiescence)
}

/// `boundary.json` first, so a crash after this point leaves a findable namespace.
fn record_boundary(dir: &Path, identity: &BoundaryIdentity, mode: SandboxMode) -> Result<()> {
    store::write_boundary(dir, identity)?;
    EventLog::open(dir)?.append(NativeEvent::BoundaryCreated { sandbox_mode: mode })?;
    Ok(())
}
