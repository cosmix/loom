pub mod attach;
pub mod auto_merge;
pub mod continuation;
pub mod core;
pub mod hooks;
pub mod monitor;
pub mod progressive_merge;
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
pub use monitor::events::RecoveryType;
pub use monitor::{
    build_failure_info, context_health, context_usage_percent, failure_state_path, heartbeat_path,
    read_heartbeat, remove_heartbeat, write_heartbeat, ContextHealth, FailureRecord,
    FailureTracker, Heartbeat, HeartbeatStatus, HeartbeatWatcher, Monitor, MonitorConfig,
    MonitorEvent, StageFailureState, DEFAULT_HEARTBEAT_POLL_SECS, DEFAULT_HUNG_TIMEOUT_SECS,
    DEFAULT_MAX_FAILURES,
};
pub use progressive_merge::{
    get_merge_point, merge_completed_stage, merge_completed_stage_with_timeout, MergeLock,
    ProgressiveMergeResult,
};
pub use signals::{
    generate_recovery_signal, generate_signal, list_signals, read_recovery_signal, read_signal,
    remove_signal, update_signal, DependencyStatus, LastHeartbeatInfo, RecoveryReason,
    RecoverySignalContent, SignalContent, SignalUpdates,
};
// Re-export crash reporting from spawner (until migrated to separate module)
pub use spawner::{generate_crash_report, get_session_log_tail, CrashReport};
// Re-export terminal functions (replaces legacy spawner exports)
pub use terminal::native::NativeBackend;
pub use terminal::{create_backend, BackendType, TerminalBackend};
// Re-export hooks infrastructure
pub use hooks::{
    generate_hooks_settings, log_hook_event, setup_hooks_for_worktree, HookEvent, HookEventLog,
    HookEventPayload, HooksConfig,
};
