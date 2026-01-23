//! Log tailing and status broadcasting threads.

use super::super::protocol::{write_message, CompletionSummary, Response};
use super::core::DaemonServer;
use super::status::collect_status;
use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::models::stage::StageStatus;

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
    let mut completion_sent = false;
    let mut orchestration_start: Option<chrono::DateTime<chrono::Utc>> = None;

    while !shutdown_flag.load(Ordering::Relaxed) {
        // Only broadcast if there are subscribers
        let has_subscribers = status_subscribers
            .lock()
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if has_subscribers {
            if let Ok(status_update) = collect_status(work_dir) {
                // Check if orchestration is complete (all stages terminal)
                if let Response::StatusUpdate {
                    ref stages_executing,
                    ref stages_pending,
                    ref stages_completed,
                    ref stages_blocked,
                } = status_update
                {
                    // Track orchestration start time from first executing stage
                    if orchestration_start.is_none() {
                        let all_stages: Vec<_> = stages_executing
                            .iter()
                            .chain(stages_completed.iter())
                            .chain(stages_blocked.iter())
                            .collect();
                        if let Some(earliest) = all_stages.iter().map(|s| s.started_at).min() {
                            orchestration_start = Some(earliest);
                        }
                    }

                    // Check if orchestration is complete
                    let total = stages_executing.len()
                        + stages_pending.len()
                        + stages_completed.len()
                        + stages_blocked.len();

                    // Orchestration is complete when:
                    // 1. No stages are executing
                    // 2. No stages are pending (waiting or queued)
                    // 3. There is at least one stage
                    let is_complete =
                        total > 0 && stages_executing.is_empty() && stages_pending.is_empty();

                    if is_complete && !completion_sent {
                        // Build completion summary
                        let all_stages: Vec<_> = stages_completed
                            .iter()
                            .chain(stages_blocked.iter())
                            .cloned()
                            .collect();

                        let success_count = all_stages
                            .iter()
                            .filter(|s| matches!(s.status, StageStatus::Completed))
                            .count();
                        let skipped_count = all_stages
                            .iter()
                            .filter(|s| matches!(s.status, StageStatus::Skipped))
                            .count();
                        let failure_count = all_stages.len() - success_count - skipped_count;

                        let total_time_secs = orchestration_start
                            .map(|start| {
                                chrono::Utc::now()
                                    .signed_duration_since(start)
                                    .num_seconds()
                            })
                            .unwrap_or(0);

                        let success = failure_count == 0;

                        let completion = Response::OrchestrationComplete {
                            summary: CompletionSummary {
                                stages: all_stages,
                                total_time_secs,
                                success_count,
                                failure_count,
                                skipped_count,
                                success,
                            },
                        };

                        if let Ok(mut subs) = status_subscribers.lock() {
                            subs.retain_mut(|stream| write_message(stream, &completion).is_ok());
                        }
                        completion_sent = true;
                    } else if !completion_sent {
                        // Send regular status update
                        if let Ok(mut subs) = status_subscribers.lock() {
                            subs.retain_mut(|stream| write_message(stream, &status_update).is_ok());
                        }
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(STATUS_BROADCAST_INTERVAL_MS));
    }
}
