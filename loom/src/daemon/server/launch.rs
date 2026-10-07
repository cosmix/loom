//! Starting the daemon from `loom run`.
//!
//! `loom run` re-executes its own binary as `loom run --daemon-child <root>`
//! plus the run's config flags, from a clean allowlisted environment, with
//! stdin on `/dev/null` and stdout and stderr on one pipe. The child writes
//! [`LOG_ACTIVE_BYTE`] to that pipe once its own output goes to
//! `orchestrator.log`, and [`READY_BYTE`] once its socket is bound. Anything
//! else it writes there (a startup error before the log redirect) is
//! diagnostic text. [`await_ready`] turns the pipe and the child's exit status
//! into the launch outcome.

use super::environment::DaemonEnvironment;
use crate::context::untrusted::terminal_safe;
use crate::daemon::DaemonConfig;
use crate::orchestrator::spawner::{clamp_from_front, read_log_tail};
use crate::orchestrator::terminal::native::detect_terminal;

use anyhow::{anyhow, Context, Result};
use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use std::io::{PipeReader, Read};
use std::os::fd::AsFd;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

mod child;
use child::{core_dumping, describe_exit, terminate, wait_for_exit};

/// Written by the daemon child once its socket is bound.
pub(super) const READY_BYTE: u8 = 0x01;
/// Written by the daemon child once its stdout and stderr go to
/// `orchestrator.log`.
pub(super) const LOG_ACTIVE_BYTE: u8 = 0x02;

const PRODUCTION_TIMING: ReadyTiming = ReadyTiming {
    deadline: Duration::from_secs(10),
    grace: Duration::from_secs(1),
};
const POLL_SLICE_MS: u16 = 50;
const POLL_SLICE: Duration = Duration::from_millis(POLL_SLICE_MS as u64);
/// How long a child whose pipe closed before the ready byte gets to exit.
const EOF_EXIT_WAIT: Duration = Duration::from_secs(1);
/// How long an exited child's pipe is read for the text it left behind. A
/// process the child started could still hold a write end, so EOF is not
/// guaranteed.
const DRAIN_LIMIT: Duration = Duration::from_secs(1);
/// The diagnostic text kept from the pipe; a flooding child keeps its tail.
const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;
/// Lines of `orchestrator.log` quoted once the child's output moved there.
const LOG_TAIL_LINES: usize = 20;

/// Defaults to disabled whenever this crate is compiled with `--cfg test`,
/// so no unit test starts a real daemon. Integration targets under
/// `loom/tests/` link the crate without `--cfg test`; one that reaches
/// [`spawn_daemon`] calls [`disable_spawn_for_tests`] first.
static SPAWN_ENABLED: AtomicBool = AtomicBool::new(!cfg!(test));

/// How many times [`spawn_daemon`] was called while [`SPAWN_ENABLED`] was
/// false, so a test can assert the guard fired without a child to observe.
#[cfg(test)]
static SUPPRESSED_SPAWNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Disable `spawn_daemon` for the rest of this process. Idempotent.
///
/// `pub` and present in every build because integration targets are separate
/// crates built without `--cfg test`.
pub fn disable_spawn_for_tests() {
    SPAWN_ENABLED.store(false, Ordering::SeqCst);
}

/// How long [`await_ready`] waits.
#[derive(Debug, Clone, Copy)]
pub struct ReadyTiming {
    /// Longest wait for the ready byte before the child is terminated.
    pub deadline: Duration,
    /// How long the child must stay alive after the ready byte.
    pub grace: Duration,
}

/// Start the daemon child for `work_dir` and wait until it is ready.
pub(crate) fn spawn_daemon(work_dir: &Path, config: &DaemonConfig) -> Result<()> {
    if !SPAWN_ENABLED.load(Ordering::SeqCst) {
        #[cfg(test)]
        SUPPRESSED_SPAWNS.fetch_add(1, Ordering::SeqCst);
        return Ok(());
    }
    let exe = std::env::current_exe().context("cannot locate the running loom binary")?;
    // `loom run` still has the operator's terminal context; the daemon does not.
    let terminal = detect_terminal()
        .ok()
        .map(|terminal| terminal.display_name());
    let environment = DaemonEnvironment::capture();
    let mut command = daemon_command(&exe, work_dir, config, &environment, terminal);
    let (reader, writer) = std::io::pipe().context("Failed to create the daemon's output pipe")?;
    let stdout = writer
        .try_clone()
        .context("Failed to duplicate the daemon's output pipe")?;
    command.stdout(stdout).stderr(writer);
    let mut child = command
        .spawn()
        .with_context(|| format!("Failed to start the daemon from {}", exe.display()))?;
    // The command keeps this process's write ends until dropped, and the pipe
    // reports EOF only once every write end is closed.
    drop(command);
    await_ready(
        &mut child,
        reader,
        &work_dir.join("orchestrator.log"),
        PRODUCTION_TIMING,
    )
}

/// The daemon child's command: `<exe> run --daemon-child <work_root>` plus
/// the config flags, started from `env` alone, with `LOOM_TERMINAL` set to
/// `terminal` when given. It never detects the terminal itself, so a host with
/// no terminal emulator can test it. The working directory is inherited: `loom
/// run` runs from the repository root, and `loom repair` matches daemons by
/// their cwd.
fn daemon_command(
    exe: &Path,
    work_root: &Path,
    config: &DaemonConfig,
    env: &DaemonEnvironment,
    terminal: Option<&str>,
) -> Command {
    let mut command = Command::new(exe);
    command.arg("run").arg("--daemon-child").arg(work_root);
    if config.manual_mode {
        command.arg("--manual");
    }
    if let Some(max_parallel) = config.max_parallel {
        command.arg("-p").arg(max_parallel.to_string());
    }
    if !config.auto_merge {
        command.arg("--no-merge");
    }
    env.apply_to(&mut command);
    if let Some(terminal) = terminal {
        command.env("LOOM_TERMINAL", terminal);
    }
    command.stdin(Stdio::null());
    command
}

/// Wait until `child` writes the ready byte (`0x01`) to `reader` and then
/// stays alive for `timing.grace`.
///
/// Every failure is an error naming what happened, followed by the text the
/// child wrote to the pipe and, once it wrote `0x02` (its output now goes to
/// the log), the last lines of `log_path`. The failures: an exit before the
/// ready byte or within the grace after it (its exit status or signal; a child
/// still dumping core when the grace ends is waited for), a pipe closed before
/// the ready byte, or no ready byte by `timing.deadline`. A
/// failure to read the pipe or check the child is an error too. On every
/// error a child still running is terminated, and the child is reaped, so a
/// failed launch leaves no daemon behind.
pub fn await_ready(
    child: &mut Child,
    reader: PipeReader,
    log_path: &Path,
    timing: ReadyTiming,
) -> Result<()> {
    let result = watch_until_ready(child, reader, log_path, timing);
    if result.is_err() {
        terminate(child);
    }
    result
}

/// [`await_ready`]'s loop. The deadline and closed-pipe failures terminate the
/// child here, before the message is built from its last output;
/// [`await_ready`] terminates it on every other error.
fn watch_until_ready(
    child: &mut Child,
    mut reader: PipeReader,
    log_path: &Path,
    timing: ReadyTiming,
) -> Result<()> {
    let started = Instant::now();
    let mut pipe = PipeState::default();
    let mut ready_at = None;
    loop {
        if pipe.eof {
            thread::sleep(POLL_SLICE);
        } else {
            pipe.pump(&mut reader)?;
        }
        if pipe.ready && ready_at.is_none() {
            ready_at = Some(Instant::now());
        }
        if let Some(status) = child
            .try_wait()
            .context("Failed to check the daemon child")?
        {
            pipe.drain(&mut reader)?;
            return Err(pipe.exit_failure(status, log_path));
        }
        match ready_at {
            Some(at) if at.elapsed() >= timing.grace => {
                return ready_unless_dumping(child, &mut pipe, &mut reader, log_path, timing);
            }
            Some(_) => {}
            None if pipe.eof => return closed_before_ready(child, &pipe, log_path),
            None if started.elapsed() >= timing.deadline => {
                terminate(child);
                pipe.drain(&mut reader)?;
                let headline = format!(
                    "the daemon did not become ready within {:?}",
                    timing.deadline
                );
                return Err(pipe.failure(headline, log_path));
            }
            None => {}
        }
    }
}

/// The pipe closed before the ready byte while the child still ran: report
/// its exit when it follows within [`EOF_EXIT_WAIT`], else terminate it.
/// Always an error.
fn closed_before_ready(child: &mut Child, pipe: &PipeState, log_path: &Path) -> Result<()> {
    if let Some(status) = wait_for_exit(child, EOF_EXIT_WAIT)? {
        return Err(pipe.exit_failure(status, log_path));
    }
    terminate(child);
    let headline = "the daemon closed its output before it was ready".to_string();
    Err(pipe.failure(headline, log_path))
}

/// The grace after the ready byte has passed: the launch succeeded unless the
/// child is dumping core. A dumping child is dying, so it is waited for up to
/// `timing.deadline` and reported like any other exit.
fn ready_unless_dumping(
    child: &mut Child,
    pipe: &mut PipeState,
    reader: &mut PipeReader,
    log_path: &Path,
    timing: ReadyTiming,
) -> Result<()> {
    if !core_dumping(child) {
        return Ok(());
    }
    if let Some(status) = wait_for_exit(child, timing.deadline)? {
        pipe.drain(reader)?;
        return Err(pipe.exit_failure(status, log_path));
    }
    let headline = format!(
        "the daemon was still dumping core {:?} after its grace",
        timing.deadline
    );
    Err(pipe.failure(headline, log_path))
}

/// What the daemon child has written to the readiness pipe so far.
#[derive(Default)]
struct PipeState {
    ready: bool,
    log_active: bool,
    eof: bool,
    text: Vec<u8>,
}

impl PipeState {
    /// Wait up to one poll slice for the pipe to become readable, then read
    /// once.
    fn pump(&mut self, reader: &mut PipeReader) -> Result<()> {
        if !readable(reader)? {
            return Ok(());
        }
        let mut buffer = [0u8; 8192];
        match reader.read(&mut buffer) {
            Ok(0) => self.eof = true,
            Ok(read) => self.absorb(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error).context("Failed to read the daemon's output"),
        }
        Ok(())
    }

    /// Read what an exited child left in the pipe, up to EOF or
    /// [`DRAIN_LIMIT`].
    fn drain(&mut self, reader: &mut PipeReader) -> Result<()> {
        let started = Instant::now();
        while !self.eof && started.elapsed() < DRAIN_LIMIT {
            self.pump(reader)?;
        }
        Ok(())
    }

    fn absorb(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match byte {
                READY_BYTE => self.ready = true,
                LOG_ACTIVE_BYTE => self.log_active = true,
                other => self.text.push(other),
            }
        }
        // Trimmed only once the buffer doubles past the cap, so a flood costs
        // one move per 64 KiB rather than one per read.
        if self.text.len() > 2 * MAX_DIAGNOSTIC_BYTES {
            let excess = self.text.len() - MAX_DIAGNOSTIC_BYTES;
            self.text.drain(..excess);
        }
    }

    fn exit_failure(&self, status: ExitStatus, log_path: &Path) -> anyhow::Error {
        let when = if self.ready {
            "right after it reported ready"
        } else {
            "before it was ready"
        };
        let headline = format!("the daemon exited {when} ({})", describe_exit(status));
        self.failure(headline, log_path)
    }

    /// `headline`, then the diagnostic text, then the log tail when the
    /// child's output had moved to the log. The log can hold agent-derived
    /// text, so its tail is made safe to print to the operator's terminal.
    fn failure(&self, headline: String, log_path: &Path) -> anyhow::Error {
        let mut message = headline;
        let text = String::from_utf8_lossy(&self.text);
        let text = clamp_from_front(text.trim(), MAX_DIAGNOSTIC_BYTES);
        if !text.is_empty() {
            message.push('\n');
            message.push_str(text);
        }
        if self.log_active {
            if let Some(tail) = read_log_tail(log_path, LOG_TAIL_LINES) {
                let tail = terminal_safe(&tail);
                message.push_str(&format!("\nlast lines of {}:\n{tail}", log_path.display()));
            }
        }
        anyhow!(message)
    }
}

/// Whether `reader` became readable (data or EOF) within one poll slice.
fn readable(reader: &PipeReader) -> Result<bool> {
    let mut fds = [PollFd::new(reader.as_fd(), PollFlags::POLLIN)];
    match poll(&mut fds, PollTimeout::from(POLL_SLICE_MS)) {
        Ok(ready) => Ok(ready > 0),
        Err(Errno::EINTR) => Ok(false),
        Err(errno) => Err(errno).context("Failed to wait on the daemon's output"),
    }
}

#[cfg(test)]
mod tests;
