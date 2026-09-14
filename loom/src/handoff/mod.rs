pub mod completion;
pub mod generator;
pub mod git_handoff;
pub mod schema;
pub mod session_content;

pub use completion::{
    attestation_key, check_definition_hash, current_blocker, expected_stage_commit,
    record_accepted_handoff, record_attempt_handoff, stage_head_commit, worktree_head_commit,
    AttestationKey, RecordOutcome, ATTESTATION_KEY_FILE, STAGE_ENVIRONMENT_POLICY,
};
pub use generator::{
    ensure_handoff, find_continuation_handoff, find_continuation_handoff_name, find_latest_handoff,
    find_latest_session_handoff, find_matching_handoff, generate_handoff, load_session_checkpoint,
    load_trusted_session_checkpoint, merge_session_handoff, HandoffContent, MergeOutcome,
};
pub use git_handoff::{format_git_history_markdown, CommitInfo, GitHistory};
pub use schema::{
    short_fingerprint, AcceptedReceipt, CommitRef, CompletedTask, CompletionAttemptEvidence,
    CompletionBlocker, CompletionCheckpoint, CompletionPhase, CriterionResult, EnvironmentFact,
    FileRef, HandoffOrigin, HandoffV2, KeyDecision, NonceObservation, ParsedHandoff,
    VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION, HANDOFF_SCHEMA_VERSION, MAX_COMMAND_LEN,
    MAX_CRITERIA, MAX_ENVIRONMENT_FACTS, MAX_EVIDENCE_BYTES, MAX_EVIDENCE_NONCES, MAX_IDENTITY_LEN,
    MAX_TEXT_LEN,
};

// Re-export continuation types from orchestrator (where they live due to spawner/signal dependencies)
pub use crate::orchestrator::continuation::{
    continue_session, load_and_parse_handoff, load_handoff_content, load_handoff_v2,
    prepare_continuation, ContinuationConfig, ContinuationContext,
};
