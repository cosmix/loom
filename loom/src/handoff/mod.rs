pub mod detector;
pub mod generator;
pub mod git_handoff;

pub use detector::{check_context_threshold, ContextLevel, ThresholdConfig};
pub use generator::{find_latest_handoff, generate_handoff, HandoffContent};
pub use git_handoff::{format_git_history_markdown, CommitInfo, GitHistory};

// Re-export continuation types from orchestrator (where they live due to spawner/signal dependencies)
pub use crate::orchestrator::continuation::{
    continue_session, load_handoff_content, prepare_continuation, ContinuationConfig,
    ContinuationContext,
};
