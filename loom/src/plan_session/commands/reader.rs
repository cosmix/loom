//! The `side-commands` thread: reads the side process's stdout line by line within its bounds,
//! hands each table on, and kills and reaps the process however reading ends, a panic included.

use std::io::{ErrorKind, PipeReader, Read};
use std::os::fd::AsFd;
use std::process::ChildStdout;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use serde_json::Value;

use super::list::CommandTable;
use super::side::{OnTable, Shared};

/// A stdout line longer than this kills the process.
const MAX_LINE: usize = 1024 * 1024;
/// More stdout than this before the `initialize` answer kills the process.
const MAX_TOTAL: usize = 8 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;
/// The longest single poll before the deadline is checked again.
const MAX_POLL: Duration = Duration::from_secs(60);

const TIMED_OUT: &str = "The command list did not arrive in time";
const LINE_TOO_LONG: &str = "The command list process printed a line over 1 MiB";
const TOO_MUCH: &str = "The command list process printed over 8 MiB before its answer";
const CLOSED: &str = "The command list process exited before its answer";
const MALFORMED: &str = "The command list answer held no commands";
const UNREADABLE: &str = "The command list process's output could not be read";

/// How reading ended.
enum End {
    /// `stop` or drop closed the wake pipe.
    Stopped,
    /// The process closed its stdout.
    Closed,
    Failed(&'static str),
}

enum Wait {
    Readable,
    Woken,
    Again,
}

/// Kills and reaps the process when the reader thread ends, by return or by panic.
struct Cleanup(Arc<Shared>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.cleanup();
    }
}

pub(super) struct Reader {
    stdout: ChildStdout,
    wake: PipeReader,
    shared: Arc<Shared>,
    on_table: OnTable,
    request_id: String,
    deadline: Instant,
    /// The last table's generation; `None` until the answer.
    generation: Option<u64>,
    /// Bytes read before the answer.
    total: usize,
}

impl Reader {
    /// A reader waiting for the answer to `request_id` until `deadline`; `wake` ends it once
    /// its write end closes.
    pub(super) fn new(
        stdout: ChildStdout,
        wake: PipeReader,
        shared: Arc<Shared>,
        on_table: OnTable,
        request_id: String,
        deadline: Instant,
    ) -> Self {
        Self {
            stdout,
            wake,
            shared,
            on_table,
            request_id,
            deadline,
            generation: None,
            total: 0,
        }
    }

    pub(super) fn run(mut self) {
        let _cleanup = Cleanup(Arc::clone(&self.shared));
        let end = self.pump();
        // The process is gone before anyone hears of it.
        self.shared.cleanup();
        if self.generation.is_none() {
            let reason = match end {
                End::Stopped => return,
                End::Closed => CLOSED,
                End::Failed(reason) => reason,
            };
            self.deliver(CommandTable::unavailable(reason));
        }
    }

    fn pump(&mut self) -> End {
        let mut chunk = vec![0; CHUNK];
        let mut pending = Vec::new();
        loop {
            let Some(timeout) = self.timeout() else {
                return End::Failed(TIMED_OUT);
            };
            match self.wait(timeout) {
                Ok(Wait::Readable) => {}
                Ok(Wait::Again) => continue,
                Ok(Wait::Woken) => return End::Stopped,
                Err(_) => return End::Failed(UNREADABLE),
            }
            let read = match self.stdout.read(&mut chunk) {
                Ok(0) => return End::Closed,
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return End::Failed(UNREADABLE),
            };
            if let Some(end) = self.take(&mut pending, &chunk[..read]) {
                return end;
            }
        }
    }

    /// The poll timeout: none once answered, else what is left of the deadline (`None` when
    /// it has passed).
    fn timeout(&self) -> Option<PollTimeout> {
        if self.generation.is_some() {
            return Some(PollTimeout::NONE);
        }
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        // Round up so a sub-millisecond remainder does not spin.
        let left = (left + Duration::from_micros(999)).min(MAX_POLL);
        Some(PollTimeout::try_from(left).unwrap_or(PollTimeout::MAX))
    }

    fn wait(&self, timeout: PollTimeout) -> nix::Result<Wait> {
        let mut fds = [
            PollFd::new(self.stdout.as_fd(), PollFlags::POLLIN),
            PollFd::new(self.wake.as_fd(), PollFlags::POLLIN),
        ];
        match poll(&mut fds, timeout) {
            Ok(0) | Err(Errno::EINTR) => return Ok(Wait::Again),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
        let woken = fds[1].revents().is_some_and(|events| !events.is_empty());
        Ok(if woken { Wait::Woken } else { Wait::Readable })
    }

    /// Append `bytes` to `pending` and handle every whole line in it, within the bounds.
    fn take(&mut self, pending: &mut Vec<u8>, bytes: &[u8]) -> Option<End> {
        if self.generation.is_none() {
            self.total += bytes.len();
            if self.total > MAX_TOTAL {
                return Some(End::Failed(TOO_MUCH));
            }
        }
        pending.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(offset) = pending[start..].iter().position(|byte| *byte == b'\n') {
            let line = &pending[start..start + offset];
            start += offset + 1;
            if line.len() > MAX_LINE {
                return Some(End::Failed(LINE_TOO_LONG));
            }
            if let Some(end) = self.line(line) {
                return Some(end);
            }
        }
        pending.drain(..start);
        (pending.len() > MAX_LINE).then_some(End::Failed(LINE_TOO_LONG))
    }

    /// Handle one stdout line; lines that are neither the answer nor `commands_changed` are
    /// ignored.
    fn line(&mut self, line: &[u8]) -> Option<End> {
        let value: Value = serde_json::from_slice(line).ok()?;
        match self.generation {
            None if self.is_answer(&value) => {
                let rows = value.pointer("/response/response/commands");
                let Some(rows) = rows.and_then(Value::as_array) else {
                    return Some(End::Failed(MALFORMED));
                };
                self.generation = Some(1);
                self.deliver(CommandTable::from_initialize(rows, 1));
            }
            Some(generation) if is_commands_changed(&value) => {
                if let Some(rows) = value["commands"].as_array() {
                    let next = generation.saturating_add(1);
                    self.generation = Some(next);
                    self.deliver(CommandTable::from_initialize(rows, next));
                }
            }
            _ => {}
        }
        None
    }

    fn is_answer(&self, value: &Value) -> bool {
        let id = value
            .pointer("/response/request_id")
            .and_then(Value::as_str);
        value["type"] == "control_response" && id == Some(self.request_id.as_str())
    }

    fn deliver(&mut self, table: CommandTable) {
        if !self.shared.stopped.load(Ordering::SeqCst) {
            (self.on_table)(Arc::new(table));
        }
    }
}

fn is_commands_changed(value: &Value) -> bool {
    value["type"] == "system" && value["subtype"] == "commands_changed"
}
