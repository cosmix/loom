//! Session attachment functionality for loom orchestrator.
//!
//! This module provides functionality to attach to running sessions,
//! list attachable sessions, and manage multi-session views.
//! Supports both tmux and native terminal backends.

mod gui;
pub(crate) mod helpers;
mod list;
pub(crate) mod loaders;
mod overview;
mod parsers;
mod single;
pub(crate) mod types;

#[cfg(test)]
mod tests;

// Re-export public API
pub use gui::{spawn_gui_windows, TerminalEmulator};
pub use list::{format_attachable_list, list_attachable};
pub use overview::{
    attach_native_all, attach_overview_session, create_overview_session, create_tiled_overview,
    print_many_sessions_warning, print_native_instructions, print_overview_instructions,
    print_tiled_instructions,
};
pub use single::{
    attach_by_session, attach_by_stage, print_attach_instructions, print_native_attach_info,
};
pub use types::{AttachableSession, SessionBackend};

// Re-export crate-internal API
pub(crate) use helpers::{
    attach_command_for_session, format_manual_mode_error, format_status, tmux_attach_command,
    try_focus_window_by_pid, window_name_for_session,
};
pub(crate) use loaders::{
    detect_backend_type, find_session_for_stage, is_attachable, load_session, load_stage,
    session_backend,
};
