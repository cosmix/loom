//! Spawning one boundary's bubblewrap and reading the namespace it reports.
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::{keep_scratch_unless_released, kill_namespace, Quiescence, SpawnedBoundary};
use crate::process::{spawn_retrying_text_busy, OwnedProcess};
use crate::verify::criteria::CargoScratch;

/// How long bubblewrap may take to report its namespace.
const INFO_TIMEOUT: Duration = Duration::from_secs(5);
/// Bound on the stderr kept from a boundary that failed to start.
const STDERR_TAIL_BYTES: u64 = 2048;

/// A start that failed after bubblewrap was spawned. Retiring the outer
/// process group does not reach the namespace's first process (it runs in
/// its own session), and without the namespace identity nothing can prove it
/// gone: the caller must not record the boundary as released, and the
/// scratch stays on disk ([`started_then_failed`]).
#[derive(Debug)]
pub(crate) struct StartedThenFailed(pub(super) String);

impl std::fmt::Display for StartedThenFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bubblewrap was started, then failed: {}", self.0)
    }
}

impl std::error::Error for StartedThenFailed {}

/// Spawn the bubblewrap `command`, which reports on `writer`'s descriptor,
/// with `scratch` bound inside it.
pub(super) fn launch(
    mut command: Command,
    scratch: CargoScratch,
    reader: std::io::PipeReader,
    writer: std::io::PipeWriter,
) -> Result<SpawnedBoundary> {
    scratch.populate()?;
    // Before bubblewrap runs, so no crash from here on leaves an unmarked
    // scratch the sweep could remove under a live namespace.
    scratch.mark_boundary(None)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    inherit_fd(&mut command, writer.as_raw_fd());
    let mut child = spawn_retrying_text_busy(&mut command).context("start bubblewrap")?;
    drop(writer);
    let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let mut process = OwnedProcess::new(child);
    let boundary_pid = process.child().id() as i32;
    let info = receive_info(reader);
    if let Ok((_, pid_namespace)) = &info {
        mark_namespace(&scratch, *pid_namespace);
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
        let detail = "bubblewrap started without its standard streams".to_string();
        return Err(abandon(process, scratch, info.ok(), detail));
    };
    match info {
        Ok((namespace_pid, pid_namespace)) => Ok(SpawnedBoundary {
            process,
            stdin,
            stdout,
            stderr,
            boundary_pid,
            namespace_pid,
            pid_namespace,
            scratch,
        }),
        Err(error) => {
            drop(stdin);
            Err(unreported(process, stderr, scratch, error))
        }
    }
}

/// Retire a boundary that reported no namespace. On an empty report the
/// scratch goes with the error ([`InfoError::NotReported`] says why that is
/// safe even when a namespace was cloned); on an ambiguous one it is kept.
fn unreported(
    mut process: OwnedProcess,
    stderr: std::process::ChildStderr,
    scratch: CargoScratch,
    error: InfoError,
) -> anyhow::Error {
    process.retire();
    let tail = stderr_tail(stderr);
    let detail = format!("{error}; bubblewrap stderr: {}", tail.trim());
    match error {
        InfoError::NotReported => anyhow!("bubblewrap failed before starting: {detail}"),
        InfoError::Ambiguous(_) => started_then_failed(scratch, detail),
    }
}

/// Retire a started boundary that cannot be used. With its namespace
/// (`namespace_pid`, inode) reported, that namespace is killed and the
/// scratch released if it is proven empty; otherwise the scratch is kept.
fn abandon(
    mut process: OwnedProcess,
    mut scratch: CargoScratch,
    namespace: Option<(i32, u64)>,
    detail: String,
) -> anyhow::Error {
    process.retire();
    let Some((namespace_pid, pid_namespace)) = namespace else {
        return started_then_failed(scratch, detail);
    };
    let quiescence = kill_namespace(namespace_pid, pid_namespace);
    keep_scratch_unless_released(&mut scratch, &quiescence);
    if quiescence == Quiescence::Released {
        return anyhow!("bubblewrap failed, its namespace emptied: {detail}");
    }
    StartedThenFailed(detail).into()
}

/// Record `pid_namespace` in the scratch marker; without it the marker keeps
/// the scratch until a reboot.
fn mark_namespace(scratch: &CargoScratch, pid_namespace: u64) {
    if let Err(error) = scratch.mark_boundary(Some(pid_namespace)) {
        tracing::warn!(%error, "cannot record the boundary's pid namespace");
    }
}

/// The [`StartedThenFailed`] error, keeping `scratch` on disk: nothing proves
/// the namespace it is bound into gone. Its marker lets the sweep reclaim it
/// once the namespace, or with none recorded the boot, is gone.
pub(super) fn started_then_failed(mut scratch: CargoScratch, detail: String) -> anyhow::Error {
    scratch.keep();
    StartedThenFailed(detail).into()
}

/// Keep `fd` open across exec in the child of `command`.
fn inherit_fd(command: &mut Command, fd: i32) {
    // SAFETY: `fcntl` is async-signal-safe; it only clears close-on-exec on
    // the inherited write end so bubblewrap can report on it.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Why bubblewrap's info JSON gave no namespace.
#[derive(Debug)]
enum InfoError {
    /// The info pipe closed with nothing on it. A namespace may still have
    /// been cloned: bubblewrap's parent can die after the clone, before it
    /// writes the info. Removing the scratch is safe because the child closes
    /// the info fd and blocks on its wait fd before it mounts anything or
    /// calls `setsid`: it is still in the outer process group, so the group
    /// SIGKILL in `OwnedProcess::retire` reaches it, and nothing is bound yet.
    /// A bubblewrap upgrade must be re-checked against this ordering.
    NotReported,
    /// Nothing within [`INFO_TIMEOUT`], or a partial or malformed report:
    /// a namespace may exist.
    Ambiguous(anyhow::Error),
}

impl std::fmt::Display for InfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReported => {
                f.write_str("bubblewrap exited before reporting its pid namespace")
            }
            Self::Ambiguous(error) => write!(f, "{error:#}"),
        }
    }
}

/// The first process's pid and the pid-namespace inode from bubblewrap's info
/// JSON, read on a helper thread for at most [`INFO_TIMEOUT`].
fn receive_info(reader: std::io::PipeReader) -> Result<(i32, u64), InfoError> {
    receive_within(reader, INFO_TIMEOUT)
}

/// A report still missing after `timeout` is ambiguous: bubblewrap may have
/// cloned the namespace and not written yet, so the scratch is kept.
fn receive_within(
    reader: impl Read + Send + 'static,
    timeout: Duration,
) -> Result<(i32, u64), InfoError> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(parse_info(reader));
    });
    receiver.recv_timeout(timeout).unwrap_or_else(|_| {
        let error = anyhow!("bubblewrap reported no pid namespace within {timeout:?}");
        Err(InfoError::Ambiguous(error))
    })
}

/// The first JSON value on `reader`: its `child-pid`, the namespace's first
/// process, and its `pid-namespace`. End of input before any value is
/// [`InfoError::NotReported`].
fn parse_info(reader: impl Read) -> Result<(i32, u64), InfoError> {
    let mut values = serde_json::Deserializer::from_reader(reader.take(64 * 1024))
        .into_iter::<serde_json::Value>();
    let info = values.next().ok_or(InfoError::NotReported)?;
    let fields = info
        .context("parse the bubblewrap info JSON")
        .and_then(|info| {
            let field = |name: &str| info.get(name).and_then(serde_json::Value::as_u64);
            let child = field("child-pid").context("bubblewrap info names no child-pid")?;
            let child =
                i32::try_from(child).context("bubblewrap info names an invalid child-pid")?;
            let namespace =
                field("pid-namespace").context("bubblewrap info names no pid-namespace")?;
            Ok((child, namespace))
        });
    fields.map_err(InfoError::Ambiguous)
}

/// At most [`STDERR_TAIL_BYTES`] of a retired boundary's stderr, waiting at
/// most one second.
pub(super) fn stderr_tail(stderr: std::process::ChildStderr) -> String {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.take(STDERR_TAIL_BYTES).read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
