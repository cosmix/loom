//! Supervisor process for one native worker: boundary, protocol loop, teardown and settlement.
mod claim;
pub mod control;
mod io;
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod lifecycle_tests;
mod preflight;
#[cfg(test)]
mod startup_tests;
mod teardown;
#[cfg(target_os = "macos")]
mod teardown_mac;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_io;
#[cfg(test)]
mod tests_stage_cache;

use std::collections::VecDeque;
use std::path::Path;
use std::process::{ChildStderr, ChildStdin, ChildStdout, ExitCode};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

pub use io::SystemClock;
pub use teardown::settle;
pub(crate) use teardown::{prove_and_reclaim, unprovable_spawn};

use self::claim::{admitted_manifest, claim, invocation_of, stage_cache_of};
use self::control::{ControlEvent, ControlPoller};
use self::io::{start_pipes, IoThreads};
use self::teardown::{settle_exit, settle_unspawned};
use super::boundary::{self, BoundarySpec, Quiescence, SpawnedBoundary};
use super::machine::{Action, Input, Machine, MachineConfig, STARTUP_DEADLINE_SECS};
use super::model::{
    BoundaryIdentity, NativeEvent, Outcome, INTERRUPT_GRACE_SECS, MAX_BRIEF_BYTES,
    PROGRESS_EVENT_INTERVAL_SECS, REAP_GRACE_SECS,
};
use super::protocol::frame::{classify, FrameError, Transport};
use super::protocol::wire::SandboxMode;
use super::store::{self, EventLog};
use super::{prompt, proxy};

/// Time source of the loop; tests step it.
pub trait Clock {
    fn now(&self) -> Instant;
}

const CONTROL_POLL: Duration = Duration::from_millis(100);
const RECV_TIMEOUT: Duration = Duration::from_millis(200);
const STARTUP: Duration = Duration::from_secs(STARTUP_DEADLINE_SECS);
const GRACE: Duration = Duration::from_secs(INTERRUPT_GRACE_SECS);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(PROGRESS_EVENT_INTERVAL_SECS);

/// Drives the reducer over `transport` until it tears down; returns the
/// teardown's outcome and reason. Any record failure is an `Err`. `mode` is
/// the boundary mode the launch confirmed (`SpawnedBoundary::activate`);
/// `thread/start` and `turn/start` carry it.
pub fn run_session<T: Transport>(
    dir: &Path,
    transport: &mut T,
    clock: &dyn Clock,
    mode: SandboxMode,
) -> Result<(Outcome, String)> {
    Session::new(dir, transport, clock, mode)?.run()
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
    startup_at: Option<Instant>,
    grace_at: Option<Instant>,
    last_poll: Option<Instant>,
    last_event: Instant,
    teardown: Option<(Outcome, String)>,
}

impl<'a, T: Transport> Session<'a, T> {
    fn new(
        dir: &'a Path,
        transport: &'a mut T,
        clock: &'a dyn Clock,
        sandbox: SandboxMode,
    ) -> Result<Self> {
        let manifest = store::read_manifest(dir).context("manifest.json")?;
        let brief = super::store_io::read_regular(dir, "brief.md", MAX_BRIEF_BYTES)
            .context("brief.md")?
            .ok_or_else(|| anyhow!("brief.md is missing"))?;
        let brief = String::from_utf8(brief).context("brief.md is not UTF-8")?;
        let machine = Machine::new(MachineConfig {
            cwd: manifest.workspace_root.clone(),
            model: manifest.model.clone(),
            effort: manifest.effort.clone(),
            task_text: prompt::task_text(&manifest.brief_path, &brief),
            client_version: env!("CARGO_PKG_VERSION").into(),
            sandbox,
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
            startup_at: None,
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
        // Checked before the receive: a handshake frame that arrives after expiry loses.
        if self.startup_at.is_some_and(|at| now >= at) {
            self.startup_at = None;
            self.feed(Input::StartupDeadline, now)?;
        }
        if self.machine.is_finished() {
            return Ok(());
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
            Action::ArmStartupDeadline => self.startup_at = Some(now + STARTUP),
            Action::ArmDeadline => {
                self.startup_at = None;
                self.deadline_at = Some(now + self.deadline);
            }
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
    let invocation = match invocation_of(dir) {
        Ok(invocation) => invocation,
        Err(e) => return Ok(settle_unspawned(dir, &format!("{e:#}"))),
    };
    let (codex_home, codex_program, loom_program) = match preflight::preflight(&manifest) {
        Ok(tools) => tools,
        Err(reason) => return Ok(settle_unspawned(dir, &reason)),
    };
    let socket_dir = match proxy::socket_dir() {
        Ok(socket_dir) => socket_dir,
        Err(e) => return Ok(settle_unspawned(dir, &format!("socket directory: {e}"))),
    };
    let proxy_socket = socket_dir.path().join(proxy::SOCKET_NAME);
    let proxy = match proxy::start(&proxy_socket, &crate::codex::CODEX_SANDBOX_DOMAINS) {
        Ok(proxy) => proxy,
        Err(e) => return Ok(settle_unspawned(dir, &format!("proxy start failed: {e:#}"))),
    };
    let stage_cache = match stage_cache_of(dir, &manifest.stage_id) {
        Ok(cache) => cache,
        Err(e) => return Ok(settle_unspawned(dir, &format!("stage cache: {e:#}"))),
    };
    let spec = BoundarySpec {
        worktree: manifest.workspace_root,
        codex_home,
        codex_program,
        proxy_socket,
        loom_program,
        stage_cache: Some(stage_cache),
        invocation,
    };
    // Durable before a boundary process can exist: a crash between the spawn
    // and `boundary.json` must not let the daemon release ownership unproven.
    if let Err(e) = store::mark_spawn_begun(dir) {
        return Ok(settle_unspawned(dir, &format!("spawn marker: {e}")));
    }
    // From this (main) thread, before any reader, writer or drain thread exists.
    let code = match boundary::spawn(&spec) {
        Ok(spawned) => supervise_boundary(dir, spawned),
        Err(e) => {
            teardown::settle_spawn_failure(dir, &format!("{e:#}"), boundary::spawn_started(&e))
        }
    };
    drop((proxy, socket_dir)); // in order: the proxy stops before its directory goes
    Ok(code)
}

/// Records the boundary, activates it, runs the session, tears the boundary
/// down and settles. Only the pipes move out of `spawned`; the rest (the
/// process, the mounted Cargo home on Linux, the control channel and reserved
/// port on macOS) lives until this function returns, after the boundary is
/// retired. A boundary not proven empty keeps its scratch on disk.
fn supervise_boundary(dir: &Path, mut spawned: SpawnedBoundary) -> ExitCode {
    let identity = match boundary::identity_for(&spawned) {
        Ok(identity) => identity,
        Err(e) => return teardown::settle_unrecorded(dir, spawned, &e),
    };
    // Codex starts only once `boundary.json` names the boundary; an
    // activation error means it never started and the protocol must not.
    let started = match record_boundary(dir, &identity) {
        Ok(()) => spawned.activate().map_err(|e| {
            let reason = format!("boundary activation failed: {e:#}");
            (Outcome::Failed, reason)
        }),
        Err(e) => Err((
            Outcome::Unknown,
            format!("could not record boundary: {e:#}"),
        )),
    };
    let (stdin, stdout, stderr) = (spawned.stdin, spawned.stdout, spawned.stderr);
    let (result, threads) = match started {
        Ok(mode) => run_pipes(dir, (stdin, stdout, stderr), mode),
        Err(settled) => {
            drop((stdin, stdout, stderr));
            (Ok(settled), None)
        }
    };
    let quiescence = teardown::retire_recorded(&mut spawned.process, &identity);
    #[cfg(target_os = "linux")]
    boundary::keep_scratch_unless_released(&mut spawned.scratch, &quiescence);
    if let (Quiescence::Released, Some(threads)) = (&quiescence, threads) {
        threads.join_within(Duration::from_secs(REAP_GRACE_SECS));
    }
    let (outcome, reason) = result.unwrap_or_else(|e| {
        (
            Outcome::Unknown,
            format!("event log or record failure: {e:#}"),
        )
    });
    settle_exit(dir, outcome, &reason, quiescence)
}

type Pipes = (ChildStdin, ChildStdout, ChildStderr);

/// Runs the session over the boundary's pipes; the threads come back still
/// running, for a bounded join once the boundary is proven empty.
fn run_pipes(
    dir: &Path,
    (stdin, stdout, stderr): Pipes,
    mode: SandboxMode,
) -> (Result<(Outcome, String)>, Option<IoThreads>) {
    match start_pipes(dir, stdin, stdout, stderr) {
        Ok(mut transport) => {
            let result = run_session(dir, &mut transport, &SystemClock, mode);
            (result, Some(transport.close_input()))
        }
        Err(e) => (Err(anyhow!(e).context("start the pipe threads")), None),
    }
}

/// `boundary.json` first, so a crash after this point leaves a findable boundary.
/// Codex's own sandbox always runs on Linux: qualification requires it to nest.
fn record_boundary(dir: &Path, identity: &BoundaryIdentity) -> Result<()> {
    store::write_boundary(dir, identity)?;
    EventLog::open(dir)?.append(NativeEvent::BoundaryCreated)?;
    Ok(())
}
