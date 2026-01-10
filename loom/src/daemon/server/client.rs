//! Client connection handling.

use super::super::protocol::{read_message, write_message, Request, Response};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Handle a client connection.
pub fn handle_client_connection(
    mut stream: UnixStream,
    shutdown_flag: Arc<AtomicBool>,
    status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
) -> Result<()> {
    loop {
        let request: Request = match read_message(&mut stream) {
            Ok(req) => req,
            Err(_) => {
                // Client disconnected or error reading
                break;
            }
        };

        match request {
            Request::Ping => {
                write_message(&mut stream, &Response::Pong)?;
            }
            Request::Stop => {
                write_message(&mut stream, &Response::Ok)?;
                shutdown_flag.store(true, Ordering::Relaxed);
                break;
            }
            Request::SubscribeStatus => {
                if let Ok(stream_clone) = stream.try_clone() {
                    match status_subscribers.lock() {
                        Ok(mut subs) => {
                            subs.push(stream_clone);
                            write_message(&mut stream, &Response::Ok)?;
                        }
                        Err(_) => {
                            write_message(
                                &mut stream,
                                &Response::Error {
                                    message: "Failed to acquire subscriber lock".to_string(),
                                },
                            )?;
                        }
                    }
                } else {
                    write_message(
                        &mut stream,
                        &Response::Error {
                            message: "Failed to clone stream".to_string(),
                        },
                    )?;
                }
            }
            Request::SubscribeLogs => {
                if let Ok(stream_clone) = stream.try_clone() {
                    match log_subscribers.lock() {
                        Ok(mut subs) => {
                            subs.push(stream_clone);
                            write_message(&mut stream, &Response::Ok)?;
                        }
                        Err(_) => {
                            write_message(
                                &mut stream,
                                &Response::Error {
                                    message: "Failed to acquire subscriber lock".to_string(),
                                },
                            )?;
                        }
                    }
                } else {
                    write_message(
                        &mut stream,
                        &Response::Error {
                            message: "Failed to clone stream".to_string(),
                        },
                    )?;
                }
            }
            Request::Unsubscribe => {
                write_message(&mut stream, &Response::Ok)?;
                break;
            }
            Request::StartWithConfig(new_config) => {
                // Log the config update request
                // Note: Config updates take effect on next daemon restart
                // The currently running orchestrator continues with its original config
                println!(
                    "Received config update: max_parallel={:?}, manual={}, watch={}, auto_merge={}",
                    new_config.max_parallel,
                    new_config.manual_mode,
                    new_config.watch_mode,
                    new_config.auto_merge
                );
                if let Some(ref stage_id) = new_config.stage_id {
                    println!("  stage_id: {stage_id}");
                }
                write_message(&mut stream, &Response::ConfigApplied)?;
            }
        }
    }

    Ok(())
}
