pub mod auto_merge;
pub mod continuation;
pub mod core;
pub mod monitor;
pub mod notify;
pub mod progressive_merge;
pub mod retry;
pub mod signals;
pub mod skip;
pub mod spawner;
pub mod terminal;

pub use auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
pub use continuation::{
    continue_session, load_handoff_content, prepare_continuation, ContinuationConfig,
    ContinuationContext,
};
pub use core::{Orchestrator, OrchestratorConfig, OrchestratorResult};
pub use monitor::{
    build_failure_info, context_health, context_usage_percent, failure_state_path, heartbeat_path,
    read_heartbeat, remove_heartbeat, write_heartbeat, ContextHealth, FailureRecord,
    FailureTracker, Heartbeat, HeartbeatStatus, HeartbeatWatcher, Monitor, MonitorConfig,
    MonitorEvent, StageFailureState, DEFAULT_HEARTBEAT_POLL_SECS, DEFAULT_HUNG_TIMEOUT_SECS,
    DEFAULT_MAX_FAILURES,
};
pub use notify::{notify_needs_human_review, send_desktop_notification};
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
pub use spawner::{generate_crash_report, CrashReport};
// Re-export terminal functions (replaces legacy spawner exports)
pub use terminal::native::NativeBackend;
pub use terminal::{create_backend, BackendType, TerminalBackend};
// Re-export hooks infrastructure from top-level hooks module
pub use crate::hooks::{
    generate_hooks_settings, log_hook_event, setup_hooks_for_worktree, HookEvent, HookEventLog,
    HookEventPayload, HooksConfig,
};
