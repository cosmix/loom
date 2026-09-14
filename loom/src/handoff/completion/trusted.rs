use crate::handoff::completion::AttestationKey;
use crate::handoff::schema::{
    CompletionBlocker, CompletionCheckpoint, NonceObservation, MAX_EVIDENCE_NONCES,
};

impl CompletionCheckpoint {
    pub fn trusted(&self, key: &AttestationKey) -> CompletionCheckpoint {
        let observations: Vec<_> = self
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

        CompletionCheckpoint {
            stage_id: self.stage_id.clone(),
            session_id: self.session_id.clone(),
            latest: latest.cloned(),
            blocker,
            first_observed_at: trusted_observed_at.clone(),
            last_observed_at: trusted_observed_at,
            capacity_exhausted: self.capacity_exhausted
                && observations.len() >= MAX_EVIDENCE_NONCES,
            observations,
            conflict: self.conflict,
            accepted: accepted.cloned(),
        }
    }
}

fn contains_nonce(observations: &[NonceObservation], nonce: &str) -> bool {
    observations
        .iter()
        .any(|observation| observation.evidence_nonce == nonce)
}
