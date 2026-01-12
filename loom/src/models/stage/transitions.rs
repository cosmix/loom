use anyhow::{bail, Result};

use super::types::StageStatus;

impl StageStatus {
    /// Check if transitioning from the current status to the new status is valid.
    ///
    /// Valid transitions:
    /// - `WaitingForDeps` -> `Queued` | `Skipped` (when dependencies satisfied or user skips)
    /// - `Queued` -> `Executing` | `Skipped` | `Blocked` (when session spawns, user skips, or pre-execution failure)
    /// - `Executing` -> `Completed` | `Blocked` | `NeedsHandoff` | `WaitingForInput` | `MergeConflict` | `CompletedWithFailures` | `MergeBlocked`
    /// - `Blocked` -> `Queued` | `Skipped` (when unblocked or user skips)
    /// - `NeedsHandoff` -> `Queued` (when resumed)
    /// - `WaitingForInput` -> `Executing` (when input provided)
    /// - `MergeConflict` -> `Completed` | `Blocked` (when conflicts resolved or resolution fails)
    /// - `CompletedWithFailures` -> `Executing` (for retry)
    /// - `MergeBlocked` -> `Executing` (for retry)
    /// - `Completed` is a terminal state
    /// - `Skipped` is a terminal state
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `true` if the transition is valid, `false` otherwise
    pub fn can_transition_to(&self, new_status: &StageStatus) -> bool {
        // Same status is always valid (no-op)
        if self == new_status {
            return true;
        }

        match self {
            StageStatus::WaitingForDeps => {
                matches!(new_status, StageStatus::Queued | StageStatus::Skipped)
            }
            StageStatus::Queued => {
                matches!(
                    new_status,
                    StageStatus::Executing | StageStatus::Skipped | StageStatus::Blocked
                )
            }
            StageStatus::Executing => matches!(
                new_status,
                StageStatus::Completed
                    | StageStatus::Blocked
                    | StageStatus::NeedsHandoff
                    | StageStatus::WaitingForInput
                    | StageStatus::MergeConflict
                    | StageStatus::CompletedWithFailures
                    | StageStatus::MergeBlocked
            ),
            StageStatus::WaitingForInput => matches!(new_status, StageStatus::Executing),
            StageStatus::Completed => false, // Terminal state
            StageStatus::Blocked => {
                matches!(new_status, StageStatus::Queued | StageStatus::Skipped)
            }
            StageStatus::NeedsHandoff => matches!(new_status, StageStatus::Queued),
            StageStatus::Skipped => false, // Terminal state
            StageStatus::MergeConflict => {
                matches!(new_status, StageStatus::Completed | StageStatus::Blocked)
            }
            StageStatus::CompletedWithFailures => {
                matches!(new_status, StageStatus::Executing)
            }
            StageStatus::MergeBlocked => {
                matches!(new_status, StageStatus::Executing)
            }
        }
    }

    /// Attempt to transition to a new status, returning an error if invalid.
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `Ok(new_status)` if the transition is valid, `Err` otherwise
    pub fn try_transition(&self, new_status: StageStatus) -> Result<StageStatus> {
        if self.can_transition_to(&new_status) {
            Ok(new_status)
        } else {
            bail!("Invalid stage status transition: {self} -> {new_status}")
        }
    }

    /// Returns the list of valid statuses this status can transition to.
    pub fn valid_transitions(&self) -> Vec<StageStatus> {
        match self {
            StageStatus::WaitingForDeps => vec![StageStatus::Queued, StageStatus::Skipped],
            StageStatus::Queued => {
                vec![
                    StageStatus::Executing,
                    StageStatus::Skipped,
                    StageStatus::Blocked,
                ]
            }
            StageStatus::Executing => vec![
                StageStatus::Completed,
                StageStatus::Blocked,
                StageStatus::NeedsHandoff,
                StageStatus::WaitingForInput,
                StageStatus::MergeConflict,
                StageStatus::CompletedWithFailures,
                StageStatus::MergeBlocked,
            ],
            StageStatus::WaitingForInput => vec![StageStatus::Executing],
            StageStatus::Completed => vec![], // Terminal state
            StageStatus::Blocked => vec![StageStatus::Queued, StageStatus::Skipped],
            StageStatus::NeedsHandoff => vec![StageStatus::Queued],
            StageStatus::Skipped => vec![], // Terminal state
            StageStatus::MergeConflict => vec![StageStatus::Completed, StageStatus::Blocked],
            StageStatus::CompletedWithFailures => vec![StageStatus::Executing],
            StageStatus::MergeBlocked => vec![StageStatus::Executing],
        }
    }
}
