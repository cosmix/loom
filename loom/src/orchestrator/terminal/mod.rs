//! Terminal session management
//!
//! Spawns and manages Claude Code sessions via [`backend::SessionBackend`],
//! which selects between the native and tmux backends according to the
//! persisted `[terminal]` config (`.loom/work/config.toml`).
//!
//! Dispatches stage, contract, merge, base-conflict, knowledge and adjudication
//! sessions through the selected backend.

pub mod backend;
pub mod emulator;
pub mod native;
mod session_tail;
pub mod tmux;

// Re-export terminal emulator
pub use backend::SessionBackend;
pub use emulator::TerminalEmulator;
pub use session_tail::session_tail;
pub use tmux::TmuxBackend;
