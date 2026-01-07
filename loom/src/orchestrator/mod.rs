pub mod attach;
pub mod continuation;
pub mod core;
pub mod monitor;
pub mod signals;
pub mod spawner;

pub use attach::{
    attach_by_session, attach_by_stage, attach_overview_session, create_overview_session,
    format_attachable_list, list_attachable, print_attach_instructions, print_overview_instructions,
    spawn_gui_windows, AttachableSession, TerminalEmulator,
};
pub use continuation::{
    continue_session, load_handoff_content, prepare_continuation, ContinuationConfig,
    ContinuationContext,
};
pub use core::{Orchestrator, OrchestratorConfig, OrchestratorResult};
pub use monitor::{
    context_health, context_usage_percent, ContextHealth, Monitor, MonitorConfig, MonitorEvent,
};
pub use signals::{
    generate_signal, list_signals, read_signal, remove_signal, update_signal, DependencyStatus,
    SignalContent, SignalUpdates,
};
pub use spawner::{
    check_tmux_available, get_tmux_session_info, kill_session, list_tmux_sessions, send_keys,
    session_is_running, spawn_session, SpawnerConfig, TmuxSessionInfo,
};
