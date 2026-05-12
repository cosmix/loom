//! Daemon server module for Unix socket-based communication.

mod broadcast;
mod client;
mod core;
mod lifecycle;
mod orchestrator;
mod status;

#[cfg(test)]
mod tests;

pub use client::{read_admin_token, read_auth_token, read_user_token};
pub use core::{DaemonServer, DaemonStatus};
pub use status::collect_completion_summary;
