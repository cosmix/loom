//! Terminal session management
//!
//! Spawns and manages Claude Code sessions in tmux via
//! [`backend::SessionBackend`]: stage, contract, merge, knowledge and
//! adjudication sessions. [`launch`] holds the launch
//! preparation and PID tracking the tmux lane runs on.

pub mod backend;
#[cfg(test)]
mod contract_tests;
pub mod launch;
mod session_tail;
pub mod tmux;

pub use backend::{require_tmux, tmux_on_path, SessionBackend, TMUX_MISSING_MESSAGE};
pub use session_tail::session_tail;
pub use tmux::TmuxBackend;
