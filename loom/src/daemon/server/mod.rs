//! Daemon server module for Unix socket-based communication.

mod broadcast;
mod client;
mod core;
mod lifecycle;
mod orchestrator;
mod status;

#[cfg(test)]
mod tests;

pub use core::DaemonServer;
