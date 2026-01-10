//! Type definitions for tmux backend

use chrono::{DateTime, Utc};

/// Debounce delay between sending text and Enter key (milliseconds)
pub const TMUX_DEBOUNCE_MS: u64 = 200;

/// Number of retry attempts for sending Enter key
pub const TMUX_ENTER_RETRY_ATTEMPTS: u32 = 3;

/// Delay between Enter key retry attempts (milliseconds)
pub const TMUX_ENTER_RETRY_DELAY_MS: u64 = 200;

/// Information about a tmux session
#[derive(Debug, Clone, PartialEq)]
pub struct TmuxSessionInfo {
    pub name: String,
    pub created: Option<DateTime<Utc>>,
    pub attached: bool,
    pub windows: u32,
}
