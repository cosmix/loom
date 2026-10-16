use anyhow::{bail, Result};

use super::types::SessionStatus;

impl SessionStatus {
    /// Self transitions are accepted as no-ops in every state.
    pub fn can_transition_to(&self, new_status: &SessionStatus) -> bool {
        self == new_status || self.outgoing_edges().contains(new_status)
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

    /// Returns outgoing state changes, excluding no-op self transitions.
    pub fn valid_transitions(&self) -> Vec<SessionStatus> {
        self.outgoing_edges().to_vec()
    }

    fn outgoing_edges(&self) -> &'static [SessionStatus] {
        use SessionStatus::*;
        match self {
            Spawning => &[Running, Crashed, ContextExhausted],
            Running => &[Completed, Paused, Crashed, ContextExhausted],
            Paused => &[Running],
            Completed | Crashed | ContextExhausted => &[],
        }
    }

    /// Returns true if this is a terminal state (no valid outgoing transitions).
    pub fn is_terminal(&self) -> bool {
        self.outgoing_edges().is_empty()
    }
}

#[cfg(test)]
mod exhaustive_policy_tests {
    use super::*;

    #[test]
    fn enforcing_api_matches_every_state_pair() {
        use SessionStatus::*;
        let states = [
            Spawning,
            Running,
            Paused,
            Completed,
            Crashed,
            ContextExhausted,
        ];
        // Bit j of row i permits the non-self edge states[i] -> states[j].
        let outgoing: [u16; 6] = [50, 60, 2, 0, 0, 0];
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
