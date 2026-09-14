//! Structured handoff schema for validated YAML handoffs.
//!
//! This module defines the V2 handoff format which uses typed YAML fields
//! instead of unstructured prose. This enables machine-readable handoffs
//! that can be validated and parsed reliably.

mod completion;
mod parsing;
mod types;
mod v2;

// Re-export all public types
pub use completion::{
    short_fingerprint, AcceptedReceipt, CompletionAttemptEvidence, CompletionBlocker,
    CompletionCheckpoint, CompletionPhase, CriterionResult, EnvironmentFact, NonceObservation,
    VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION, MAX_COMMAND_LEN, MAX_CRITERIA,
    MAX_ENVIRONMENT_FACTS, MAX_EVIDENCE_BYTES, MAX_EVIDENCE_NONCES, MAX_IDENTITY_LEN, MAX_TEXT_LEN,
};
pub use parsing::ParsedHandoff;
pub use types::{CommitRef, CompletedTask, FileRef, KeyDecision};
pub use v2::{HandoffOrigin, HandoffV2, HANDOFF_SCHEMA_VERSION};
