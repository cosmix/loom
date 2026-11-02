//! The HTTP and WebSocket layer shared by loom's browser-facing servers.
//!
//! [`serve`] runs one accept loop over a bound listener and hands every
//! request that passes the shared gates - read/write timeouts, a complete
//! request head, and the `Host` check of [`access::AccessPolicy`] - to an
//! injected [`Router`]. What a server answers is the router's business; how
//! connections are admitted, bounded, refused and drained is this module's.
//!
//! No response written here carries an `Access-Control-Allow-*` header, and
//! every one carries the fixed security headers and Content-Security-Policy of
//! [`http`].

pub mod access;
pub mod assets;
pub mod auth;
pub(crate) mod bootstrap;
pub(crate) mod connection;
pub(crate) mod head;
pub mod http;
pub mod hub;
pub(crate) mod interfaces;
pub mod limits;
pub mod listener;
mod serve;
mod shutdown;
pub(crate) mod token;
mod unserved;
pub(crate) mod ws;

pub use serve::{serve, ConnInfo, Router, ServeConfig};
pub use shutdown::ShutdownPending;
