pub mod detector;
pub mod generator;

pub use detector::{
    check_context_threshold, context_status_message, context_usage_percent, should_handoff,
    ContextLevel, ThresholdConfig,
};
pub use generator::{
    detect_current_branch, find_latest_handoff, generate_handoff, get_modified_files,
    HandoffContent,
};

// Re-export continuation types from orchestrator (where they live due to spawner/signal dependencies)
pub use crate::orchestrator::continuation::{
    continue_session, load_handoff_content, prepare_continuation, ContinuationConfig,
    ContinuationContext,
};
