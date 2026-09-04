//! Log tailing and status broadcasting threads.

use super::super::protocol::{write_message, Response};
use super::super::wire::MAX_RESPONSE_BYTES;
use super::core::DaemonServer;
use super::status::{collect_completion_summary, collect_status};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Interval between status broadcasts in milliseconds.
///
/// Set to 1s (P-1): the prior 200ms cadence drove a full stage rescan plus git
/// subprocesses five times a second for the daemon's entire life — even with
/// zero subscribers, since the emptiness check happened *after* collection.
/// Status updates are now skipped entirely when no one is subscribed (see
/// `run_status_broadcaster`), and 1s is ample for a live TUI.
const STATUS_BROADCAST_INTERVAL_MS: u64 = 1000;

/// Interval between log file rotation checks in iterations.
const LOG_ROTATION_CHECK_INTERVAL: u32 = 50; // ~5 seconds at 100ms sleep

/// Spawn the log tailing thread.
///
/// Returns a join handle if the log file exists and the thread was spawned.
pub fn spawn_log_tailer(server: &DaemonServer) -> Option<JoinHandle<()>> {
    if !server.log_path.exists() {
        return None;
    }

    let log_path = server.log_path.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);
    let log_subscribers = Arc::clone(&server.log_subscribers);

    Some(thread::spawn(move || {
        if let Err(e) = run_log_tailer(&log_path, shutdown_flag, log_subscribers) {
            eprintln!("Log tailer error: {e}");
        }
    }))
}

/// Run the log tailer loop (static method for thread).
fn run_log_tailer(
    log_path: &Path,
    shutdown_flag: Arc<AtomicBool>,
    log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
) -> Result<()> {
    let (mut reader, mut current_inode) = open_log_file(log_path)?;
    let mut line = String::new();
    let mut iteration_count: u32 = 0;
    let mut oversized_logged = false;

    while !shutdown_flag.load(Ordering::Relaxed) {
        // Periodically check for log rotation or truncation
        iteration_count = iteration_count.wrapping_add(1);
        if iteration_count.is_multiple_of(LOG_ROTATION_CHECK_INTERVAL) {
            if let Some((new_reader, new_inode)) =
                check_log_rotation(log_path, &mut reader, current_inode)?
            {
                reader = new_reader;
                current_inode = new_inode;
            }
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // No new data, sleep briefly
                thread::sleep(Duration::from_millis(100));
            }
            Ok(_) => {
                let response = Response::LogLine {
                    line: line.trim_end().to_string(),
                };
                broadcast_to_subscribers(&log_subscribers, &response, &mut oversized_logged);
            }
            Err(e) => {
                eprintln!("Error reading log file: {e}");
                break;
            }
        }
    }

    Ok(())
}

/// Open log file and return reader seeked to end, along with inode.
fn open_log_file(log_path: &Path) -> Result<(BufReader<File>, u64)> {
    let log_file = File::open(log_path).context("Failed to open log file for tailing")?;
    let inode = log_file.metadata()?.ino();
    let mut reader = BufReader::new(log_file);
    reader.seek(SeekFrom::End(0))?;
    Ok((reader, inode))
}

/// Check if log file was rotated (inode changed) or truncated (size < position).
/// Returns new reader and inode if rotation/truncation detected.
fn check_log_rotation(
    log_path: &Path,
    reader: &mut BufReader<File>,
    current_inode: u64,
) -> Result<Option<(BufReader<File>, u64)>> {
    let metadata = match fs::metadata(log_path) {
        Ok(m) => m,
        Err(_) => return Ok(None), // File may be temporarily unavailable during rotation
    };

    let new_inode = metadata.ino();
    let file_size = metadata.len();
    let current_pos = reader.stream_position().unwrap_or(0);

    // Check for rotation (inode changed) or truncation (file smaller than position)
    if new_inode != current_inode || file_size < current_pos {
        eprintln!(
            "Log file rotated/truncated (inode: {current_inode} -> {new_inode}, size: {file_size}, pos: {current_pos}), reopening"
        );
        match open_log_file(log_path) {
            Ok((new_reader, new_inode)) => Ok(Some((new_reader, new_inode))),
            Err(_) => Ok(None), // File may not exist yet after rotation
        }
    } else {
        Ok(None)
    }
}

/// Spawn the status broadcasting thread.
pub fn spawn_status_broadcaster(server: &DaemonServer) -> JoinHandle<()> {
    let work_dir = server.work_dir.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);
    let status_subscribers = Arc::clone(&server.status_subscribers);

    thread::spawn(move || {
        run_status_broadcaster(&work_dir, shutdown_flag, status_subscribers);
    })
}

/// What the broadcaster loop remembers between ticks.
///
/// Both fields are one-shot latches rather than counters: the completion
/// summary is sent once, and the oversized-payload line is logged once per
/// entry into the condition. Without the second latch a payload that stays over
/// the frame limit writes one line per second into `orchestrator.log`, which
/// only rotates when the daemon restarts.
#[derive(Default)]
struct BroadcastState {
    completion_sent: bool,
    oversized_logged: bool,
}

/// Run the status broadcaster loop (static method for thread).
fn run_status_broadcaster(
    work_dir: &Path,
    shutdown_flag: Arc<AtomicBool>,
    status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
) {
    let completion_marker_path = work_dir.join("orchestrator.complete");
    let mut state = BroadcastState::default();

    while !shutdown_flag.load(Ordering::Relaxed) {
        // P-1: short-circuit BEFORE collecting status. `collect_status` re-parses
        // every stage file and spawns several git subprocesses per worktree; doing
        // that work and then discarding it because nobody is subscribed wasted
        // 20-60% of a core, 24/7. Acquire the subscriber lock only briefly to test
        // emptiness, then release it before any I/O.
        let has_subscribers = {
            let subs = lock_or_recover(&status_subscribers);
            !subs.is_empty()
        };

        if !has_subscribers {
            thread::sleep(Duration::from_millis(STATUS_BROADCAST_INTERVAL_MS));
            continue;
        }

        // Collect data outside of the lock to minimize lock hold time. Only
        // reached when at least one subscriber is connected.
        let completion_response = if !state.completion_sent && completion_marker_path.exists() {
            collect_completion_summary(work_dir).ok().map(|summary| {
                state.completion_sent = true;
                Response::OrchestrationComplete { summary }
            })
        } else {
            None
        };

        let status_response = collect_status(work_dir).ok();

        // Single lock acquisition for all broadcasts
        let subscriber_count = {
            let mut subs = lock_or_recover(&status_subscribers);
            if subs.is_empty() {
                // A subscriber may have disconnected between the emptiness check
                // and now; nothing to send.
                0
            } else {
                // Send completion notification if we have one
                if let Some(ref response) = completion_response {
                    broadcast_retaining_live(&mut subs, response, &mut state.oversized_logged);
                }

                // Send regular status update
                if let Some(ref response) = status_response {
                    broadcast_retaining_live(&mut subs, response, &mut state.oversized_logged);
                }

                subs.len()
            }
        };

        // Log completion outside the lock
        if completion_response.is_some() {
            println!("Orchestration complete - notified {subscriber_count} subscriber(s)");
        }

        thread::sleep(Duration::from_millis(STATUS_BROADCAST_INTERVAL_MS));
    }

    // Final completion check before exiting - handles race with shutdown_flag
    if !state.completion_sent && completion_marker_path.exists() {
        if let Ok(summary) = collect_completion_summary(work_dir) {
            let response = Response::OrchestrationComplete { summary };
            let mut subs = lock_or_recover(&status_subscribers);
            broadcast_retaining_live(&mut subs, &response, &mut state.oversized_logged);
            println!(
                "Orchestration complete (final) - notified {} subscriber(s)",
                subs.len()
            );
        }
    }
}

/// Broadcast a response to all subscribers, removing any that fail.
fn broadcast_to_subscribers(
    subscribers: &Arc<Mutex<Vec<UnixStream>>>,
    response: &Response,
    oversized_logged: &mut bool,
) {
    let mut subs = lock_or_recover(subscribers);
    broadcast_retaining_live(&mut subs, response, oversized_logged);
}

/// Send `response` to every subscriber, dropping only those whose own stream
/// failed to write.
///
/// The frame limit is checked once, up front, because `write_json_frame`
/// refuses an oversized frame BEFORE writing any bytes and returns the same
/// `Err` a dead peer returns. Retaining on `write_message(..).is_ok()` alone
/// therefore evicted EVERY subscriber over what is the daemon's own bug — and
/// evicted each dashboard again the moment it reconnected, so no live view
/// could recover until the payload shrank on its own.
///
/// A response that will not fit is replaced by a [`Response::Error`] carrying
/// the size and the cap, not silently dropped: the dashboard reads
/// `tick_age_secs` off `orchestrator.tick` rather than off the broadcast, so a
/// skipped tick otherwise leaves the header reporting a healthy daemon above
/// rows that have stopped moving. The TUI routes `Response::Error` into the
/// footer. The notice is a short constant-shaped string, so it fits by
/// construction and no second size check — and no recursion — is needed.
fn broadcast_retaining_live(
    subscribers: &mut Vec<UnixStream>,
    response: &Response,
    oversized_logged: &mut bool,
) {
    let Some(notice) = frame_overflow(response) else {
        *oversized_logged = false;
        subscribers.retain_mut(|stream| write_message(stream, response).is_ok());
        return;
    };

    // Log on entry into the condition only: it holds for as long as the payload
    // stays oversized, and `orchestrator.log` rotates only on daemon restart.
    if !*oversized_logged {
        eprintln!("{notice}");
        *oversized_logged = true;
    }

    let notice = Response::Error { message: notice };
    subscribers.retain_mut(|stream| write_message(stream, &notice).is_ok());
}

/// The operator-facing reason `response` cannot be broadcast, or `None` when it
/// serializes within the wire's response frame.
fn frame_overflow(response: &Response) -> Option<String> {
    match serde_json::to_vec(response) {
        Ok(json) if json.len() <= MAX_RESPONSE_BYTES => None,
        Ok(json) => Some(format!(
            "Broadcast skipped: {} bytes exceeds the {MAX_RESPONSE_BYTES} byte frame limit",
            json.len()
        )),
        Err(error) => Some(format!(
            "Broadcast skipped: failed to serialize response: {error}"
        )),
    }
}

/// Lock a mutex, recovering from poison if necessary.
/// Logs a warning if the mutex was poisoned but continues with the data.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned: PoisonError<_>| {
        eprintln!("Warning: mutex was poisoned (another thread panicked), recovering");
        poisoned.into_inner()
    })
}

#[cfg(test)]
#[path = "broadcast_tests.rs"]
mod tests;
