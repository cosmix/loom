//! Log tailing and status broadcasting threads.

use super::super::protocol::{write_message, Response};
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
/// Reduced to 200ms for faster shutdown detection after orchestration completes.
const STATUS_BROADCAST_INTERVAL_MS: u64 = 200;

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

    while !shutdown_flag.load(Ordering::Relaxed) {
        // Periodically check for log rotation or truncation
        iteration_count = iteration_count.wrapping_add(1);
        if iteration_count % LOG_ROTATION_CHECK_INTERVAL == 0 {
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
                broadcast_to_subscribers(&log_subscribers, &response);
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

/// Run the status broadcaster loop (static method for thread).
fn run_status_broadcaster(
    work_dir: &Path,
    shutdown_flag: Arc<AtomicBool>,
    status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
) {
    let completion_marker_path = work_dir.join("orchestrator.complete");
    let mut completion_sent = false;

    while !shutdown_flag.load(Ordering::Relaxed) {
        // Collect data outside of lock to minimize lock hold time
        let completion_response = if !completion_sent && completion_marker_path.exists() {
            collect_completion_summary(work_dir).ok().map(|summary| {
                completion_sent = true;
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
                0
            } else {
                // Send completion notification if we have one
                if let Some(ref response) = completion_response {
                    subs.retain_mut(|stream| write_message(stream, response).is_ok());
                }

                // Send regular status update
                if let Some(ref response) = status_response {
                    subs.retain_mut(|stream| write_message(stream, response).is_ok());
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
}

/// Broadcast a response to all subscribers, removing any that fail.
fn broadcast_to_subscribers(subscribers: &Arc<Mutex<Vec<UnixStream>>>, response: &Response) {
    let mut subs = lock_or_recover(subscribers);
    subs.retain_mut(|stream| write_message(stream, response).is_ok());
}

/// Lock a mutex, recovering from poison if necessary.
/// Logs a warning if the mutex was poisoned but continues with the data.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned: PoisonError<_>| {
        eprintln!("Warning: mutex was poisoned (another thread panicked), recovering");
        poisoned.into_inner()
    })
}
