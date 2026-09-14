mod attest;
mod checkpoint;
mod current;
pub(crate) mod identity;
mod record;

pub use attest::{attestation_key, AttestationKey, ATTESTATION_KEY_FILE};
pub use checkpoint::RecordOutcome;
pub use current::current_blocker;
pub use identity::{
    check_definition_hash, expected_stage_commit, stage_head_commit, worktree_head_commit,
    STAGE_ENVIRONMENT_POLICY,
};
pub use record::{record_accepted_handoff, record_attempt_handoff};

#[cfg(test)]
mod attest_tests;
#[cfg(test)]
mod checkpoint_tests;
#[cfg(test)]
mod checkpoint_trust_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod shared_tests;
