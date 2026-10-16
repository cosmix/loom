use anyhow::{bail, Result};

use super::types::StageStatus;

impl StageStatus {
    /// Self transitions are accepted as no-ops in every state.
    pub fn can_transition_to(&self, new_status: &StageStatus) -> bool {
        self == new_status || self.outgoing_edges().contains(new_status)
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

    /// Returns outgoing state changes, excluding no-op self transitions.
    pub fn valid_transitions(&self) -> Vec<StageStatus> {
        self.outgoing_edges().to_vec()
    }

    fn outgoing_edges(&self) -> &'static [StageStatus] {
        use StageStatus::*;
        match self {
            WaitingForDeps => &[Queued, Skipped],
            Queued => &[Executing, Skipped, Blocked],
            Executing => &[
                Completed,
                Blocked,
                NeedsHandoff,
                WaitingForInput,
                MergeConflict,
                CompletedWithFailures,
                MergeBlocked,
                NeedsHumanReview,
                NeedsAdjudication,
            ],
            WaitingForInput => &[Executing],
            Completed => &[], // Terminal state
            Blocked => &[Queued, Skipped, NeedsHandoff],
            NeedsHandoff => &[Queued],
            Skipped => &[], // Terminal state
            MergeConflict => &[Completed, Blocked],
            CompletedWithFailures => &[
                Queued,
                Executing,
                Completed,
                MergeConflict,
                MergeBlocked,
                NeedsAdjudication,
                NeedsHumanReview,
            ],
            MergeBlocked => &[Queued, Executing, Completed, MergeConflict],
            NeedsHumanReview => &[Executing, Completed, Blocked, Queued],
            NeedsAdjudication => &[Queued, NeedsHumanReview],
        }
    }
}

#[cfg(test)]
mod exhaustive_policy_tests {
    use super::*;

    #[test]
    fn enforcing_api_matches_every_state_pair() {
        use StageStatus::*;
        let states = [
            WaitingForDeps,
            Queued,
            Executing,
            Completed,
            Blocked,
            NeedsHandoff,
            Skipped,
            WaitingForInput,
            MergeConflict,
            CompletedWithFailures,
            MergeBlocked,
            NeedsHumanReview,
            NeedsAdjudication,
        ];
        // Bit j of row i permits the non-self edge states[i] -> states[j].
        let outgoing: [u16; 13] = [66, 84, 8120, 0, 98, 2, 0, 4, 24, 7438, 270, 30, 2050];
        for (i, from) in states.iter().enumerate() {
            assert!(
                !from.valid_transitions().contains(from),
                "self is a no-op, not an edge"
            );
            for (j, to) in states.iter().enumerate() {
                let expected = i == j || outgoing[i] & (1 << j) != 0;
                assert_eq!(
                    from.try_transition(to.clone()).is_ok(),
                    expected,
                    "{from} -> {to}"
                );
                assert_eq!(from.can_transition_to(to), expected);
                assert_eq!(from.valid_transitions().contains(to), i != j && expected);
            }
        }
    }
}
