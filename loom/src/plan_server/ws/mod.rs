//! The planning server's duplex WebSocket: the hub that routes messages to
//! open connections and the loop that serves each one. The upgrade gate
//! (Origin, cookies) runs before this module sees a stream.

pub mod conn;
pub mod dispatch;
pub mod hub;

#[cfg(test)]
mod tests;

pub use conn::run_connection;
pub use hub::PlanHub;

/// Largest inbound WebSocket message or frame, in bytes (section 12).
pub const MAX_INBOUND_BYTES: usize = 256 * 1024;
