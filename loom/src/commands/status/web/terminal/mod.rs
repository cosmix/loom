//! Browser terminals: a PTY-hosted `tmux attach-session` client per WebSocket.
mod bridge;
mod history;
mod protocol;
mod pty;
mod resolve;
#[cfg(test)]
mod tests_bridge;
#[cfg(test)]
mod tests_pty;
#[cfg(test)]
mod tests_route;
#[cfg(test)]
mod tests_scroll;
#[cfg(test)]
pub(in crate::commands::status::web) mod tests_upgrade;
pub(in crate::commands::status::web) mod token;
mod upgrade;
pub(in crate::commands::status::web) use history::{
    serve as serve_history, ROUTE_PREFIX as HISTORY_PREFIX,
};
use protocol::{Mode, WindowSize, CLOSE_NOT_YET, CLOSE_REFUSED, CLOSE_UNKNOWN_STAGE};
pub(crate) use upgrade::handle_upgrade;
