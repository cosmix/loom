//! Log tailing and status broadcasting threads.

use super::super::protocol::{write_message, Response};
use super::core::DaemonServer;
use super::status::{collect_completion_summary, collect_status};
use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Interval between status broadcasts in milliseconds.
const STATUS_BROADCAST_INTERVAL_MS: u64 = 1000;

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
    let log_file = fs::File::open(log_path).context("Failed to open log file for tailing")?;
    let mut reader = BufReader::new(log_file);

    // Seek to end of file to only tail new content
    reader.seek(SeekFrom::End(0))?;

    let mut line = String::new();

    while !shutdown_flag.load(Ordering::Relaxed) {
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
                if let Ok(mut subs) = log_subscribers.lock() {
                    subs.retain_mut(|stream| write_message(stream, &response).is_ok());
                }
            }
            Err(e) => {
                eprintln!("Error reading log file: {e}");
                break;
            }
        }
    }

    Ok(())
}

/// Spawn the status broadcasting thread.
pub fn spawn_status_broadcaster(server: &DaemonServer) -> Option<JoinHandle<()>> {
    let work_dir = server.work_dir.clone();
    let shutdown_flag = Arc::clone(&server.shutdown_flag);
    let status_subscribers = Arc::clone(&server.status_subscribers);

    Some(thread::spawn(move || {
        run_status_broadcaster(&work_dir, shutdown_flag, status_subscribers);
    }))
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
        // Only broadcast if there are subscribers
        let has_subscribers = status_subscribers
            .lock()
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if has_subscribers {
            // Check for completion marker (only send once)
            if !completion_sent && completion_marker_path.exists() {
                if let Ok(summary) = collect_completion_summary(work_dir) {
                    let completion_response = Response::OrchestrationComplete { summary };
                    if let Ok(mut subs) = status_subscribers.lock() {
                        subs.retain_mut(|stream| {
                            write_message(stream, &completion_response).is_ok()
                        });
                    }
                    completion_sent = true;

                    // Log completion to console
                    println!(
                        "Orchestration complete - notified {} subscriber(s)",
                        status_subscribers.lock().map(|s| s.len()).unwrap_or(0)
                    );
                }
            }

            // Continue sending regular status updates
            if let Ok(status_update) = collect_status(work_dir) {
                if let Ok(mut subs) = status_subscribers.lock() {
                    subs.retain_mut(|stream| write_message(stream, &status_update).is_ok());
                }
            }
        }

        thread::sleep(Duration::from_millis(STATUS_BROADCAST_INTERVAL_MS));
    }
}
