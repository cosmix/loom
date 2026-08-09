//! Daemon server module for Unix socket-based communication.

mod admission;
mod broadcast;
mod client;
mod core;
mod dispute;
mod environment;
mod lifecycle;
mod lock;
mod orchestrator;
mod pool;
mod shutdown;
mod status;
mod storage;

#[cfg(test)]
mod tests;

pub use client::{admin_token_path, read_auth_token, read_user_token};
pub use core::{DaemonServer, DaemonStatus};
pub use dispute::handle_dispute_criteria;
pub(crate) use shutdown::DaemonUnavailable;
pub use status::collect_completion_summary;
