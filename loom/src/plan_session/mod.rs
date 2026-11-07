//! Planning sessions of loom plan: kinds, the per-session sandbox settings and the PreToolUse
//! guard, and the session bridge: the chat model, transcript mapping, hook spool and permission
//! answers, the session state machine, launch and tmux input, and the `Bridge` that runs them.

pub mod attachments;
pub mod bridge;
pub mod guard;
pub mod hook_cli;
pub mod input;
pub mod kind;
pub mod launch;
pub mod model;
pub mod owner_watch;
pub mod permission;
pub mod sandbox;
pub mod scope;
pub mod spool;
pub mod state;
pub mod tmux_host;
pub mod tmux_io;
pub mod transcript;
pub mod web_fetch;
