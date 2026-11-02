//! Running one worker: the payload on stdin from a private file, a scratch
//! working directory, a scrubbed environment, its own process group, a
//! deadline, and an interrupt handler that kills the group.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use crate::context::untrusted::inline_safe;
use crate::process::OwnedProcess;

/// Captured stdout past this is a failure.
pub(crate) const STDOUT_CAP: usize = 16 * 1024 * 1024;
/// Captured stderr only feeds an error excerpt; the rest is drained unread.
const STDERR_CAP: usize = 64 * 1024;
/// Characters of stderr's tail quoted in a failure.
const STDERR_EXCERPT_CHARS: usize = 400;
const STDOUT_EXCERPT_CHARS: usize = 300;

/// Session variables of an enclosing agent session that must not reach the
/// worker, beside every `LOOM_*` variable.
const SESSION_VARS: [&str; 7] = [
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
];

/// The running worker's process group, or 0 when none runs.
static WORKER_GROUP: AtomicI32 = AtomicI32::new(0);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Install the one SIGINT/SIGTERM handler. The worker sits in its own process
/// group, so a terminal's signal never reaches it: the handler kills that
/// group and marks the run interrupted.
pub(crate) fn install_interrupt_handler() -> Result<()> {
    ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::SeqCst);
        kill_group(WORKER_GROUP.load(Ordering::SeqCst));
    })
    .context("failed to install the interrupt handler")
}

/// Whether SIGINT or SIGTERM arrived.
pub(crate) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

fn kill_group(group: i32) {
    if group > 0 {
        let _ = kill(Pid::from_raw(-group), Signal::SIGKILL);
    }
}

/// Publishes the worker's group to the handler while it runs.
struct TrackedGroup;

impl TrackedGroup {
    fn track(pid: u32) -> Self {
        let group = i32::try_from(pid).unwrap_or(0);
        WORKER_GROUP.store(group, Ordering::SeqCst);
        // A signal that landed before the store found no group to kill.
        if interrupted() {
            kill_group(group);
        }
        TrackedGroup
    }
}

impl Drop for TrackedGroup {
    fn drop(&mut self) {
        WORKER_GROUP.store(0, Ordering::SeqCst);
    }
}

/// What to run and what to feed it.
pub(crate) struct Worker<'a> {
    pub program: &'a Path,
    pub args: &'a [String],
    pub env: &'a [(&'a str, &'a str)],
    pub payload: &'a str,
    pub timeout: Duration,
}

/// A worker's stdout after a zero exit, and how long it ran.
#[derive(Debug)]
pub(crate) struct Finished {
    pub stdout: String,
    pub latency_ms: u64,
}

struct Capture {
    bytes: Vec<u8>,
    overflowed: bool,
}

type Reader = Receiver<std::io::Result<Capture>>;

/// Run `worker` to completion. The group is retired on every exit path,
/// including a zero exit, because a descendant can outlive the leader while
/// holding both pipes; the readers are then collected within the retirement
/// deadline.
pub(crate) fn run_worker(worker: &Worker<'_>) -> Result<Finished> {
    if interrupted() {
        bail!("interrupted");
    }
    let scratch = tempfile::TempDir::new().context("failed to create the worker's directory")?;
    let mut command = build_command(worker, scratch.path())?;
    let started = Instant::now();
    let child = command
        .spawn()
        .with_context(|| format!("failed to start {}", worker.program.display()))?;
    let mut child = OwnedProcess::new(child);
    let stdout = start_reader(child.child().stdout.take(), STDOUT_CAP)?;
    let stderr = start_reader(child.child().stderr.take(), STDERR_CAP)?;
    let tracked = TrackedGroup::track(child.child().id());
    let status = child
        .wait(worker.timeout)
        .context("failed to wait for the worker");
    drop(tracked);
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let deadline = child.retire();
    let stdout = receive(&stdout, deadline);
    let stderr = receive(&stderr, deadline);
    if interrupted() {
        bail!("interrupted");
    }
    let Some(status) = status? else {
        bail!("worker timed out after {} s", worker.timeout.as_secs());
    };
    let stdout = finish(status, stdout, stderr)?;
    Ok(Finished { stdout, latency_ms })
}

fn build_command(worker: &Worker<'_>, dir: &Path) -> Result<Command> {
    let stdin = payload_file(dir, worker.payload)?;
    let mut command = Command::new(worker.program);
    command
        .args(worker.args)
        .current_dir(dir)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for (key, _) in std::env::vars_os() {
        if is_scrubbed(&key) {
            command.env_remove(&key);
        }
    }
    command.envs(worker.env.iter().copied());
    Ok(command)
}

/// The payload in a `0600` file opened as the child's stdin: no writer
/// thread, and a worker that never reads stdin cannot block loom.
fn payload_file(dir: &Path, payload: &str) -> Result<File> {
    let path = dir.join("payload");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(payload.as_bytes())
        .and_then(|()| file.seek(SeekFrom::Start(0)).map(drop))
        .context("failed to write the worker's payload")?;
    Ok(file)
}

fn is_scrubbed(key: &OsStr) -> bool {
    key.as_bytes().starts_with(b"LOOM_") || SESSION_VARS.iter().any(|name| key == OsStr::new(name))
}

fn start_reader(pipe: Option<impl Read + Send + 'static>, cap: usize) -> Result<Reader> {
    let mut pipe = pipe.context("worker pipe was not attached")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("loom-ask-reader".to_owned())
        .spawn(move || {
            let _ = sender.send(read_capped(&mut pipe, cap));
        })
        .context("failed to start a worker output reader")?;
    Ok(receiver)
}

/// Read to the end, keeping at most `cap` bytes and draining the rest so the
/// worker never blocks on a full pipe.
fn read_capped(pipe: &mut impl Read, cap: usize) -> std::io::Result<Capture> {
    let mut capture = Capture {
        bytes: Vec::new(),
        overflowed: false,
    };
    let mut chunk = [0u8; 8192];
    loop {
        let read = match pipe.read(&mut chunk) {
            Ok(0) => return Ok(capture),
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let keep = read.min(cap - capture.bytes.len());
        capture.bytes.extend_from_slice(&chunk[..keep]);
        capture.overflowed |= keep < read;
    }
}

fn receive(reader: &Reader, deadline: Instant) -> Option<std::io::Result<Capture>> {
    reader
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

fn finish(
    status: ExitStatus,
    stdout: Option<std::io::Result<Capture>>,
    stderr: Option<std::io::Result<Capture>>,
) -> Result<String> {
    if !status.success() {
        let mut excerpt = stderr
            .and_then(Result::ok)
            .map(|capture| stderr_excerpt(&capture.bytes))
            .unwrap_or_default();
        if excerpt.is_empty() {
            // The claude CLI reports its error result as JSON on stdout.
            excerpt = stdout
                .and_then(Result::ok)
                .map(|capture| stdout_excerpt(&capture.bytes))
                .unwrap_or_default();
        }
        bail!("worker failed ({status}): {excerpt}");
    }
    let stdout = stdout
        .context("worker output stayed open past cleanup")?
        .context("failed to read the worker's output")?;
    if stdout.overflowed {
        bail!("worker output exceeded the {} MiB cap", STDOUT_CAP >> 20);
    }
    String::from_utf8(stdout.bytes).context("worker output is not UTF-8")
}

/// The start of stdout, where an error result carries its message.
fn stdout_excerpt(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    inline_safe(&text.chars().take(STDOUT_EXCERPT_CHARS).collect::<String>())
}

fn stderr_excerpt(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end();
    let start = text
        .char_indices()
        .rev()
        .nth(STDERR_EXCERPT_CHARS - 1)
        .map_or(0, |(index, _)| index);
    inline_safe(&text[start..])
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
