pub mod attach;
pub mod auto_merge;
pub mod continuation;
pub mod core;
pub mod monitor;
pub mod retry;
pub mod signals;
pub mod skip;
pub mod spawner;
pub mod terminal;

pub use attach::{
    attach_by_session, attach_by_stage, attach_native_all, attach_overview_session,
    create_overview_session, create_tiled_overview, format_attachable_list, list_attachable,
    print_attach_instructions, print_many_sessions_warning, print_native_attach_info,
    print_native_instructions, print_overview_instructions, print_tiled_instructions,
    spawn_gui_windows, AttachableSession, SessionBackend, TerminalEmulator,
};
pub use auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
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
// Re-export crash reporting from spawner (until migrated to separate module)
pub use spawner::{generate_crash_report, get_session_log_tail, CrashReport};
// Re-export terminal functions (replaces legacy spawner exports)
pub use terminal::native::NativeBackend;
pub use terminal::tmux::{
    check_tmux_available, get_tmux_session_info, list_tmux_sessions, send_keys, session_is_running,
    TmuxBackend, TmuxSessionInfo,
};
pub use terminal::{create_backend, BackendType, TerminalBackend};
