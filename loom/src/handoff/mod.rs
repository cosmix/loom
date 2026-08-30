pub mod generator;
pub mod git_handoff;
pub mod schema;

pub use generator::{
    ensure_handoff, find_continuation_handoff, find_continuation_handoff_name, find_latest_handoff,
    find_latest_session_handoff, find_matching_handoff, generate_handoff, HandoffContent,
};
pub use git_handoff::{format_git_history_markdown, CommitInfo, GitHistory};
pub use schema::{
    CommitRef, CompletedTask, FileRef, HandoffOrigin, HandoffV2, KeyDecision, ParsedHandoff,
    HANDOFF_SCHEMA_VERSION,
};

// Re-export continuation types from orchestrator (where they live due to spawner/signal dependencies)
pub use crate::orchestrator::continuation::{
    continue_session, load_and_parse_handoff, load_handoff_content, load_handoff_v2,
    prepare_continuation, ContinuationConfig, ContinuationContext,
};
