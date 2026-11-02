use crate::handoff::completion::AttestationKey;
use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint, NonceObservation,
    MAX_EVIDENCE_NONCES,
};

impl CompletionCheckpoint {
    /// What a reader may act on: `authenticated`, minus the evidence of an
    /// overflow, which has no observation behind it, with signed time alone
    /// ordering the result.
    pub fn trusted(&self, key: &AttestationKey) -> CompletionCheckpoint {
        let mut trusted = self.authenticated(key);
        if trusted.capacity_exhausted {
            trusted.latest = None;
            trusted.blocker = None;
        }
        normalize_observation_bounds(&mut trusted.observations, trusted.latest.as_ref());
        let observed_at = trusted
            .latest
            .as_ref()
            .map(|evidence| evidence.observed_at.clone());
        trusted.first_observed_at.clone_from(&observed_at);
        trusted.last_observed_at = observed_at;
        trusted
    }

    /// The components of this one artifact its MACs prove, for a lossy merge:
    /// nothing unsigned can then displace signed state or crowd it out. Conflict
    /// and capacity are recomputed from those components. The observed-at bounds
    /// stay as written so a rewrite of unchanged evidence compares equal; no MAC
    /// covers them and `trusted` discards them.
    pub(crate) fn authenticated(&self, key: &AttestationKey) -> CompletionCheckpoint {
        let observations: Vec<_> = self
            .observations
            .iter()
            .filter(|observation| {
                key.verify_observation(&self.stage_id, &self.session_id, observation)
            })
            .cloned()
            .collect();
        let signed_latest = self
            .latest
            .as_ref()
            .filter(|evidence| key.verify_evidence(evidence));
        let observed = signed_latest
            .is_some_and(|evidence| contains_nonce(&observations, &evidence.evidence_nonce));
        // Signed evidence with no room among full observations is an overflow.
        let capacity_exhausted =
            signed_latest.is_some() && !observed && observations.len() >= MAX_EVIDENCE_NONCES;
        let latest = signed_latest.filter(|_| observed || capacity_exhausted);
        let accepted = self.accepted.as_ref().filter(|receipt| {
            key.verify_receipt(&self.stage_id, &self.session_id, receipt)
                && contains_nonce(&observations, &receipt.evidence_nonce)
        });
        let blocker = latest
            .filter(|evidence| evidence.is_actionable())
            .and_then(|evidence| CompletionBlocker::from_attempt(evidence).ok());
        let conflict = latest.is_some_and(|evidence| {
            observations.iter().any(|observation| {
                observation.evidence_nonce == evidence.evidence_nonce
                    && super::evidence_disagrees(observation, evidence)
            })
        });

        CompletionCheckpoint {
            stage_id: self.stage_id.clone(),
            session_id: self.session_id.clone(),
            latest: latest.cloned(),
            blocker,
            first_observed_at: self.first_observed_at.clone(),
            last_observed_at: self.last_observed_at.clone(),
            capacity_exhausted,
            observations,
            conflict,
            accepted: accepted.cloned(),
        }
    }

    /// Whether anything of this checkpoint is left to fold once authenticated.
    pub(crate) fn holds_signed_state(&self) -> bool {
        !self.observations.is_empty() || self.accepted.is_some()
    }
}

fn normalize_observation_bounds(
    observations: &mut [NonceObservation],
    latest: Option<&CompletionAttemptEvidence>,
) {
    // Observation bounds are not covered by the MAC. Only signed evidence
    // supplies an ordering timestamp; other observations get a neutral bound.
    for observation in observations {
        let observed_at = latest
            .filter(|evidence| evidence.evidence_nonce == observation.evidence_nonce)
            .map_or("1970-01-01T00:00:00Z", |evidence| {
                evidence.observed_at.as_str()
            });
        observation.first_observed_at = observed_at.to_owned();
        observation.last_observed_at = observed_at.to_owned();
    }
}

fn contains_nonce(observations: &[NonceObservation], nonce: &str) -> bool {
    observations
        .iter()
        .any(|observation| observation.evidence_nonce == nonce)
}
