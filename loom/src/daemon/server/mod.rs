//! Daemon server module for Unix socket-based communication.

mod admission;
mod broadcast;
mod client;
mod control_block;
mod core;
mod dispute;
mod environment;
mod lifecycle;
mod lock;
mod orchestrator;
mod peer_identity;
mod pool;
mod self_service;
mod shutdown;
mod status;
mod storage;
mod tokens;

#[cfg(test)]
mod tests;

pub(crate) use control_block::handle_block_stage;
pub use core::{DaemonServer, DaemonStatus};
pub use dispute::handle_dispute_criteria;
pub(crate) use shutdown::DaemonUnavailable;
pub use status::collect_completion_summary;
pub use tokens::{admin_token_path, read_auth_token, read_user_token};
