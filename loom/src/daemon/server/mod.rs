//! Daemon server module for Unix socket-based communication.

mod broadcast;
mod client;
mod core;
mod dispute;
mod lifecycle;
mod orchestrator;
mod status;

#[cfg(test)]
mod tests;

pub use client::{admin_token_path, read_admin_token, read_auth_token, read_user_token};
pub use core::{DaemonServer, DaemonStatus};
pub use dispute::handle_dispute_criteria;
pub use status::collect_completion_summary;
