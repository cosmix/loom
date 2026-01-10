pub mod detector;
pub mod generator;
pub mod git_handoff;
pub mod schema;

pub use detector::{check_context_threshold, ContextLevel, ThresholdConfig};
pub use generator::{find_latest_handoff, generate_handoff, HandoffContent};
pub use git_handoff::{format_git_history_markdown, CommitInfo, GitHistory};
pub use schema::{
    CommitRef, CompletedTask, FileRef, HandoffV2, KeyDecision, ParsedHandoff,
    HANDOFF_SCHEMA_VERSION,
};

// Re-export continuation types from orchestrator (where they live due to spawner/signal dependencies)
pub use crate::orchestrator::continuation::{
    continue_session, load_and_parse_handoff, load_handoff_content, load_handoff_v2,
    prepare_continuation, ContinuationConfig, ContinuationContext,
};
