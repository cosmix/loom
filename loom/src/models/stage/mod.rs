mod checks;
mod defaults;
pub mod dispute_budgets;
mod implementers;
mod issue_history;
mod merge_block;
mod methods;
mod persisted;
pub mod phases;
mod sandbox_config;
mod status_display;
mod transitions;
mod types;
mod verdict_completion;
mod verification_status;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod phases_contract_tests;

// The frozen contract builds its stage by assigning `status` to `Stage::default()`.
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod adjudication_reason_contract_tests;

#[cfg(test)]
mod phases_tests;

pub use checks::{AcceptanceCriterion, PlanIdentity, TruthCheck, WiringCheck};
pub use dispute_budgets::DisputeTally;
pub use implementers::{Implementer, Implementers};
pub use issue_history::{legacy_disposition, IssueDisposition, IssueKind, StageIssue};
pub use merge_block::MergeRecord;
pub use sandbox_config::{
    CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig, StageSandboxConfig,
};
pub use types::{
    DeadCodeCheck, ExecutionMode, PermissionMode, RegressionTest, Stage, StageOutput, StageStatus,
    StageType, StatusBucket, SuccessCriteria, WiringTest, ALLOWED_REASONING_EFFORTS,
};

/// Default timeout before a stage session is considered hung.
pub const DEFAULT_HUNG_TIMEOUT_SECS: u64 = 300;
