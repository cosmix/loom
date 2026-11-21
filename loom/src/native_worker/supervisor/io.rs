//! Supervisor I/O: the system clock, the capped provider log, the pipe transport
//! (reader and writer threads) and the stderr drain.

use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{ChildStderr, ChildStdin, ChildStdout};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::Clock;
use crate::native_worker::model::{MAX_FRAME_BYTES, MAX_RECORD_BYTES};
use crate::native_worker::protocol::frame::{read_frame, write_frame, FrameError, Transport};
use crate::process::lines::read_bounded_line;

/// Frames read ahead of the loop before the reader blocks.
const READ_QUEUE: usize = 256;
/// Frames queued for the provider before `send` reports the input as stuck.
const WRITE_QUEUE: usize = 64;
/// Bytes of provider stderr copied to the supervisor's own stderr.
const STDERR_CAP: usize = 4 * 1024 * 1024;
const CAPPED_MARKER: &str = r#"{"loom":"provider log capped"}"#;
const OVERSIZED_MARKER: &str = r#"{"loom":"oversized frame"}"#;

/// The wall clock of a real run.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// `provider.log`: raw provider frames, diagnostics only, capped in size.
pub struct CappedLog {
    file: File,
    written: u64,
    cap: u64,
    capped: bool,
}

impl CappedLog {
    /// Opens `<dir>/provider.log` for appending, capped at `MAX_RECORD_BYTES`.
    pub fn open(dir: &Path) -> io::Result<Self> {
        Self::open_with_cap(dir, MAX_RECORD_BYTES)
    }

    pub(super) fn open_with_cap(dir: &Path, cap: u64) -> io::Result<Self> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join("provider.log"))?;
        let written = file.metadata()?.len();
        Ok(Self {
            file,
            written,
            cap,
            capped: false,
        })
    }

    /// Appends one line; once the cap would be passed, writes one marker line
    /// and nothing after it.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        if self.capped {
            return Ok(());
        }
        if self.written + line.len() as u64 + 1 > self.cap {
            self.capped = true;
            return writeln!(self.file, "{CAPPED_MARKER}");
        }
        writeln!(self.file, "{line}")?;
        self.written += line.len() as u64 + 1;
        Ok(())
    }
}

/// The threads behind a closed transport. Join them only once the boundary's
/// namespace is proven empty: a surviving member may hold a pipe open.
pub struct IoThreads {
    handles: Vec<JoinHandle<()>>,
}

impl IoThreads {
    fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.push(handle);
    }

    #[cfg(test)]
    pub fn join(self) {
        for handle in self.handles {
            let _ = handle.join();
        }
    }

    /// Joins the threads whose pipes closed within `bound` and leaves the
    /// rest detached: a process no census could see (an escaped one, plan
    /// Decisions 9) may hold a pipe open forever, even when the table showed
    /// the boundary empty. A detached thread ends with the supervisor.
    pub fn join_within(self, bound: Duration) {
        let deadline = Instant::now() + bound;
        while self.handles.iter().any(|handle| !handle.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        for handle in self.handles {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

/// `Transport` over the provider's stdout and stdin, each owned by a thread so
/// that a provider that stops reading blocks only the writer, never the loop.
pub struct PipeTransport {
    frames: Receiver<Result<Value, FrameError>>,
    input: SyncSender<Value>,
    threads: IoThreads,
}

impl PipeTransport {
    pub fn start<R, W>(output: R, input: W, log: Option<CappedLog>) -> io::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (frame_tx, frames) = mpsc::sync_channel(READ_QUEUE);
        let (input_tx, input_rx) = mpsc::sync_channel(WRITE_QUEUE);
        let reader = thread::Builder::new()
            .name("native-reader".into())
            .spawn(move || read_loop(BufReader::new(output), log, frame_tx))?;
        let writer = thread::Builder::new()
            .name("native-writer".into())
            .spawn(move || write_loop(input, input_rx))?;
        Ok(Self {
            frames,
            input: input_tx,
            threads: IoThreads {
                handles: vec![reader, writer],
            },
        })
    }

    /// Ends the provider's input (the EOF the app-server exits on) and hands
    /// back the threads, still running until their pipes close.
    pub fn close_input(self) -> IoThreads {
        self.threads
    }
}

impl Transport for PipeTransport {
    fn send(&mut self, frame: &Value) -> io::Result<()> {
        match self.input.try_send(frame.clone()) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "provider input queue is full: the provider stopped reading",
            )),
            Err(TrySendError::Disconnected(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "provider input writer stopped",
            )),
        }
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<Value>, FrameError> {
        match self.frames.recv_timeout(timeout) {
            Ok(Ok(frame)) => Ok(Some(frame)),
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(FrameError::Eof),
        }
    }
}

fn read_loop<R: Read>(
    mut output: BufReader<R>,
    mut log: Option<CappedLog>,
    frames: SyncSender<Result<Value, FrameError>>,
) {
    loop {
        let result = read_frame(&mut output, MAX_FRAME_BYTES);
        if let Some(log) = log.as_mut() {
            // provider.log is diagnostics only; a failed write must not stop the reader.
            let _ = match &result {
                Ok(frame) => log.write_line(&frame.to_string()),
                Err(FrameError::Oversized) => log.write_line(OVERSIZED_MARKER),
                Err(_) => Ok(()),
            };
        }
        let last = matches!(result, Err(FrameError::Eof | FrameError::Io(_)));
        if frames.send(result).is_err() || last {
            return;
        }
    }
}

fn write_loop<W: Write>(mut input: W, frames: Receiver<Value>) {
    while let Ok(frame) = frames.recv() {
        if write_frame(&mut input, &frame).is_err() {
            return;
        }
    }
}

/// Starts the transport over a boundary's pipes, logging frames to
/// `provider.log`, plus the stderr drain. Both are diagnostics: when either is
/// unavailable the run goes on without it.
pub fn start_pipes(
    dir: &Path,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> io::Result<PipeTransport> {
    let log = CappedLog::open(dir)
        .map_err(|e| eprintln!("provider.log unavailable: {e}"))
        .ok();
    let mut transport = PipeTransport::start(stdout, stdin, log)?;
    match drain_stderr(stderr) {
        Ok(handle) => transport.threads.push(handle),
        Err(e) => eprintln!("stderr drain unavailable: {e}"),
    }
    Ok(transport)
}

/// Copies provider stderr lines to this process's stderr, prefixed `codex: `,
/// up to `STDERR_CAP` bytes; the rest is read and discarded so the provider
/// never blocks on a full stderr pipe.
fn drain_stderr<R: Read + Send + 'static>(stderr: R) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("native-stderr".into())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            let mut copied = 0usize;
            while let Ok(true) = read_bounded_line(&mut reader, &mut line, MAX_FRAME_BYTES) {
                if copied >= STDERR_CAP {
                    continue;
                }
                copied += line.len() + 1;
                // eprintln! would panic once stderr fails; diagnostics may be lost instead.
                let text = String::from_utf8_lossy(&line);
                let _ = writeln!(io::stderr(), "codex: {text}");
            }
        })
}
