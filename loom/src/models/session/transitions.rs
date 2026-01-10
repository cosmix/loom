use anyhow::{bail, Result};

use super::types::SessionStatus;

impl SessionStatus {
    /// Check if transitioning from the current status to the new status is valid.
    ///
    /// Valid transitions:
    /// - `Spawning` -> `Running`
    /// - `Running` -> `Completed` | `Paused` | `Crashed` | `ContextExhausted`
    /// - `Paused` -> `Running`
    ///
    /// Terminal states (no outgoing transitions):
    /// - `Completed`
    /// - `Crashed`
    /// - `ContextExhausted`
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `true` if the transition is valid, `false` otherwise
    pub fn can_transition_to(&self, new_status: &SessionStatus) -> bool {
        // Same status is always valid (no-op)
        if self == new_status {
            return true;
        }

        match self {
            SessionStatus::Spawning => matches!(new_status, SessionStatus::Running),
            SessionStatus::Running => matches!(
                new_status,
                SessionStatus::Completed
                    | SessionStatus::Paused
                    | SessionStatus::Crashed
                    | SessionStatus::ContextExhausted
            ),
            SessionStatus::Paused => matches!(new_status, SessionStatus::Running),
            // Terminal states
            SessionStatus::Completed | SessionStatus::Crashed | SessionStatus::ContextExhausted => {
                false
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
    pub fn try_transition(&self, new_status: SessionStatus) -> Result<SessionStatus> {
        if self.can_transition_to(&new_status) {
            Ok(new_status)
        } else {
            bail!("Invalid session status transition: {self} -> {new_status}")
        }
    }

    /// Returns the list of valid statuses this status can transition to.
    pub fn valid_transitions(&self) -> Vec<SessionStatus> {
        match self {
            SessionStatus::Spawning => vec![SessionStatus::Running],
            SessionStatus::Running => vec![
                SessionStatus::Completed,
                SessionStatus::Paused,
                SessionStatus::Crashed,
                SessionStatus::ContextExhausted,
            ],
            SessionStatus::Paused => vec![SessionStatus::Running],
            SessionStatus::Completed | SessionStatus::Crashed | SessionStatus::ContextExhausted => {
                vec![]
            }
        }
    }

    /// Returns true if this is a terminal state (no valid outgoing transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SessionStatus::Completed | SessionStatus::Crashed | SessionStatus::ContextExhausted
        )
    }
}
