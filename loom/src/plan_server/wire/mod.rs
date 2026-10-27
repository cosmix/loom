//! Wire model of the planning server: JSON bodies and WebSocket frames.

pub mod checklist;
pub mod message;
pub mod paths;
pub mod presence;
pub mod server;

#[cfg(test)]
mod paths_tests;
