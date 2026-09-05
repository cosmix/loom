//! Codex quota polling over the `codex app-server` JSON-RPC subprocess.

use crate::context::untrusted::inline_safe;
use crate::quota::model::{self, ProviderQuota, QuotaWindow, WindowKind};
use anyhow::{anyhow, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

/// A single JSON-RPC line from `codex app-server` is never trusted past this
/// size; the remainder of an over-long line is discarded without blocking
/// the writer.
const MAX_LINE_BYTES: usize = 64 * 1024;

/// How often the reply-wait loop re-checks the deadline and shutdown flag.
const RECV_SLICE: Duration = Duration::from_millis(250);

/// Wall-clock bound on a clean exit before the process group is killed.
const CHILD_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

/// Extract a [`ProviderQuota`] from the `result` of an
/// `account/rateLimits/read` JSON-RPC reply.
///
/// `rateLimits.primary`/`.secondary` are each nullable and classified by
/// `windowDurationMins` (300 -> five-hour, 10080 -> seven-day; anything else
/// ignored). A missing `rateLimits` yields zero windows.
pub fn parse_snapshot(result: &serde_json::Value, now: i64) -> ProviderQuota {
    let rate_limits = result.get("rateLimits");

    let mut five_hour: Option<QuotaWindow> = None;
    let mut seven_day: Option<QuotaWindow> = None;
    if let Some(rate_limits) = rate_limits {
        if let Some(window) = extract_window(rate_limits.get("primary")) {
            assign(&mut five_hour, &mut seven_day, window);
        }
        if let Some(window) = extract_window(rate_limits.get("secondary")) {
            assign(&mut five_hour, &mut seven_day, window);
        }
    }

    let plan = rate_limits
        .and_then(|rl| rl.get("planType"))
        .and_then(|v| v.as_str())
        .map(inline_safe);

    ProviderQuota {
        observed_at: now,
        windows: [five_hour, seven_day].into_iter().flatten().collect(),
        plan,
        error: None,
    }
}

fn assign(
    five_hour: &mut Option<QuotaWindow>,
    seven_day: &mut Option<QuotaWindow>,
    window: QuotaWindow,
) {
    match window.kind {
        WindowKind::FiveHour => {
            five_hour.get_or_insert(window);
        }
        WindowKind::SevenDay => {
            seven_day.get_or_insert(window);
        }
    }
}

fn extract_window(value: Option<&serde_json::Value>) -> Option<QuotaWindow> {
    let value = value.filter(|v| !v.is_null())?;
    let minutes = value.get("windowDurationMins").and_then(|v| v.as_i64())?;
    let kind = match minutes {
        300 => WindowKind::FiveHour,
        10080 => WindowKind::SevenDay,
        _ => return None,
    };
    let used_percent = model::clamp_percent(value.get("usedPercent").and_then(|v| v.as_f64()))?;
    let resets_at = value
        .get("resetsAt")
        .and_then(|v| v.as_i64())
        .map(model::normalize_epoch);
    Some(QuotaWindow {
        kind,
        used_percent,
        resets_at,
    })
}

/// Run one `codex app-server` exchange over a fresh subprocess: send
/// `initialize`, `initialized`, then `account/rateLimits/read`, and wait for
/// the reply to the last request.
///
/// The child is killed as a process group on every exit path (success,
/// error, deadline, or `shutdown` flipping true) so a hung or misbehaving
/// `codex` binary never outlives the poll that spawned it.
pub fn poll_once(
    codex_bin: &Path,
    deadline: Duration,
    shutdown: &AtomicBool,
    now: i64,
) -> Result<ProviderQuota> {
    let mut command = Command::new(codex_bin);
    command.arg("app-server");
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = spawn_retrying_text_busy(&mut command)
        .with_context(|| format!("failed to spawn {}", codex_bin.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .context("codex app-server: missing stdin")?;
    let stdout = child
        .stdout
        .take()
        .context("codex app-server: missing stdout")?;

    let write_result = write_requests(&mut stdin);
    let (receiver, reader_handle) = spawn_reader(stdout);

    let outcome = match write_result {
        Ok(()) => await_reply(&receiver, deadline, shutdown, now),
        Err(e) => Err(e),
    };

    // Dropped before the join below: `spawn_reader`'s channel has a capacity
    // of 16, and once `await_reply` stops draining it a chatty child (more
    // than 16 lines after the reply) blocks the reader thread inside
    // `sender.send`, which would hang this join forever. Dropping the
    // receiver first disconnects the channel, so a blocked `send` fails
    // immediately and the reader thread exits on its own.
    drop(receiver);
    teardown(child, stdin, reader_handle);
    outcome
}

/// `Command::spawn` briefly, and rarely, fails with `ETXTBSY` ("text file
/// busy") when the target binary is still settling right after being
/// written - e.g. a `codex` self-update in progress, or (in tests) a fake
/// script written moments earlier. A handful of short retries clears it
/// without treating it as a real spawn failure.
fn spawn_retrying_text_busy(command: &mut Command) -> std::io::Result<Child> {
    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_millis(20);

    for attempt in 1..=MAX_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(e) if attempt < MAX_ATTEMPTS && e.raw_os_error() == Some(libc::ETXTBSY) => {
                thread::sleep(RETRY_DELAY);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns on its final attempt")
}

fn write_requests(stdin: &mut ChildStdin) -> Result<()> {
    let version = env!("LOOM_VERSION");
    let frames = [
        serde_json::json!({
            "id": 0,
            "method": "initialize",
            "params": { "clientInfo": { "name": "loom", "title": "loom", "version": version } }
        }),
        serde_json::json!({ "method": "initialized", "params": {} }),
        serde_json::json!({ "id": 1, "method": "account/rateLimits/read", "params": {} }),
    ];
    for frame in &frames {
        writeln!(stdin, "{frame}").context("failed to write to codex app-server stdin")?;
    }
    stdin
        .flush()
        .context("failed to flush codex app-server stdin")
}

fn spawn_reader(stdout: std::process::ChildStdout) -> (mpsc::Receiver<String>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::sync_channel(16);
    let handle = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        while let Ok(true) = read_bounded_line(&mut reader, &mut buf, MAX_LINE_BYTES) {
            let line = String::from_utf8_lossy(&buf).into_owned();
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    (receiver, handle)
}

/// Read one newline-terminated line, capping retained bytes at `max_bytes`
/// and discarding (without buffering) whatever overruns the cap - so an
/// over-long line still drains from the pipe instead of blocking the writer.
/// `Ok(false)` at EOF with nothing pending.
fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_bytes: usize,
) -> std::io::Result<bool> {
    buf.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }
        if let Some(newline_pos) = available.iter().position(|&b| b == b'\n') {
            let line_part = &available[..newline_pos];
            let room = max_bytes.saturating_sub(buf.len());
            buf.extend_from_slice(&line_part[..line_part.len().min(room)]);
            let consumed = newline_pos + 1;
            reader.consume(consumed);
            return Ok(true);
        }
        let room = max_bytes.saturating_sub(buf.len());
        let take = available.len().min(room);
        buf.extend_from_slice(&available[..take]);
        let consumed = available.len();
        reader.consume(consumed);
    }
}

fn await_reply(
    receiver: &mpsc::Receiver<String>,
    deadline: Duration,
    shutdown: &AtomicBool,
    now: i64,
) -> Result<ProviderQuota> {
    let deadline_instant = Instant::now() + deadline;
    loop {
        if shutdown.load(Ordering::SeqCst) || Instant::now() >= deadline_instant {
            return Err(anyhow!("codex app-server timed out"));
        }
        match receiver.recv_timeout(RECV_SLICE) {
            Ok(line) => {
                if let Some(outcome) = handle_line(&line, now) {
                    return outcome;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("codex app-server closed without replying"));
            }
        }
    }
}

/// `Some(outcome)` when `line` is the reply to request id 1 (a result or an
/// error); `None` for anything else (garbage, notifications, the id-0 reply),
/// which `await_reply` keeps waiting past.
fn handle_line(line: &str, now: i64) -> Option<Result<ProviderQuota>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let id = value.get("id").and_then(|v| v.as_i64())?;
    if id != 1 {
        return None;
    }
    if let Some(result) = value.get("result") {
        return Some(Ok(parse_snapshot(result, now)));
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("codex app-server error");
        return Some(Err(anyhow!("{}", inline_safe(message))));
    }
    None
}

fn teardown(mut child: Child, stdin: ChildStdin, reader_handle: JoinHandle<()>) {
    drop(stdin);
    let exited = matches!(child.wait_timeout(CHILD_WAIT_TIMEOUT), Ok(Some(_)));
    if !exited {
        #[cfg(unix)]
        if let Ok(pid) = i32::try_from(child.id()) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(-pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        #[cfg(not(unix))]
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = reader_handle.join();
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
