use crate::handoff::completion::AttestationKey;
use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint, NonceObservation,
    MAX_EVIDENCE_NONCES,
};

impl CompletionCheckpoint {
    pub fn trusted(&self, key: &AttestationKey) -> CompletionCheckpoint {
        let mut observations: Vec<_> = self
            .observations
            .iter()
            .filter(|observation| {
                key.verify_observation(&self.stage_id, &self.session_id, observation)
            })
            .cloned()
            .collect();
        let latest = self.latest.as_ref().filter(|evidence| {
            key.verify_evidence(evidence) && contains_nonce(&observations, &evidence.evidence_nonce)
        });
        let accepted = self.accepted.as_ref().filter(|receipt| {
            key.verify_receipt(&self.stage_id, &self.session_id, receipt)
                && contains_nonce(&observations, &receipt.evidence_nonce)
        });
        let blocker = latest
            .filter(|evidence| evidence.is_actionable())
            .and_then(|evidence| CompletionBlocker::from_attempt(evidence).ok());
        let trusted_observed_at = latest.map(|evidence| evidence.observed_at.clone());
        normalize_observation_bounds(&mut observations, latest);
        let conflict = latest.is_some_and(|evidence| {
            observations.iter().any(|observation| {
                observation.evidence_nonce == evidence.evidence_nonce
                    && observation.identity_digest != evidence.identity_digest()
            })
        });

        CompletionCheckpoint {
            stage_id: self.stage_id.clone(),
            session_id: self.session_id.clone(),
            latest: latest.cloned(),
            blocker,
            first_observed_at: trusted_observed_at.clone(),
            last_observed_at: trusted_observed_at,
            capacity_exhausted: observations.len() >= MAX_EVIDENCE_NONCES,
            observations,
            conflict,
            accepted: accepted.cloned(),
        }
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
