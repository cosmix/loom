use anyhow::{bail, Result};
use chrono::{DateTime, Utc};

use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionBlocker, CompletionCheckpoint,
    CompletionPhase, NonceObservation, VerificationCheckpoint, MAX_EVIDENCE_NONCES,
};

#[path = "trusted.rs"]
mod trusted;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Recorded,
    PhaseUpdated,
    Duplicate,
    Conflict,
    CapacityExhausted,
}

impl CompletionCheckpoint {
    pub fn record_attempt(
        &mut self,
        evidence: &CompletionAttemptEvidence,
    ) -> Result<RecordOutcome> {
        evidence.validate()?;
        self.ensure_identity(&evidence.stage_id, &evidence.session_id)?;
        let blocker = CompletionBlocker::from_attempt(evidence).ok();
        let fingerprint = blocker.as_ref().map(|value| value.fingerprint.clone());

        let outcome = match self.observation_index(&evidence.evidence_nonce) {
            Some(index) => self.update_observation(index, evidence, fingerprint.clone())?,
            None if self.observations.len() >= MAX_EVIDENCE_NONCES => {
                self.capacity_exhausted = true;
                return Ok(RecordOutcome::CapacityExhausted);
            }
            None => {
                self.observations
                    .push(new_observation(evidence, fingerprint));
                sort_observations(&mut self.observations)?;
                RecordOutcome::Recorded
            }
        };
        if matches!(
            outcome,
            RecordOutcome::Recorded | RecordOutcome::PhaseUpdated
        ) {
            self.apply_attempt(evidence, blocker)?;
        }
        Ok(outcome)
    }

    pub fn merge_from(&mut self, other: &CompletionCheckpoint) -> Result<bool> {
        self.ensure_identity(&other.stage_id, &other.session_id)?;
        self.validate()?;
        other.validate()?;
        let before = self.clone();
        for observation in &other.observations {
            self.merge_observation(observation)?;
        }
        self.conflict |= other.conflict;
        self.capacity_exhausted |= other.capacity_exhausted;
        self.merge_latest(other)?;
        self.merge_blocker(other);
        self.merge_accepted(other);
        merge_bound(
            &mut self.first_observed_at,
            other.first_observed_at.as_ref(),
            true,
        )?;
        merge_bound(
            &mut self.last_observed_at,
            other.last_observed_at.as_ref(),
            false,
        )?;
        if *self != before {
            sort_observations(&mut self.observations)?;
        }
        Ok(*self != before)
    }

    pub fn record_accepted(&mut self, receipt: AcceptedReceipt) -> Result<()> {
        if !self
            .observations
            .iter()
            .any(|item| item.evidence_nonce == receipt.evidence_nonce)
        {
            bail!("accepted receipt refers to an unknown evidence nonce");
        }
        match &self.accepted {
            Some(existing) if existing != &receipt => {
                bail!("a different completion receipt is already recorded")
            }
            Some(_) => Ok(()),
            None => {
                self.accepted = Some(receipt);
                Ok(())
            }
        }
    }

    pub fn repeat_count(&self) -> u32 {
        let Some(blocker) = &self.blocker else {
            return 0;
        };
        self.observations
            .iter()
            .filter(|item| item.fingerprint.as_ref() == Some(&blocker.fingerprint))
            .count() as u32
    }

    pub fn is_actionable(&self) -> bool {
        if self.blocker.is_none()
            || self.conflict
            || self.capacity_exhausted
            || self.accepted.is_some()
        {
            return false;
        }
        let Some(latest) = self.latest.as_ref().filter(|item| item.is_actionable()) else {
            return false;
        };
        let Ok(latest_blocker) = CompletionBlocker::from_attempt(latest) else {
            return false;
        };
        self.blocker.as_ref().map(|value| &value.fingerprint) == Some(&latest_blocker.fingerprint)
    }

    pub fn current_phase(&self) -> Option<CompletionPhase> {
        self.latest.as_ref().map(|evidence| evidence.phase)
    }

    pub fn current_verification(&self) -> Option<&VerificationCheckpoint> {
        self.latest.as_ref().map(|evidence| &evidence.verification)
    }

    fn ensure_identity(&self, stage_id: &str, session_id: &str) -> Result<()> {
        if self.stage_id != stage_id || self.session_id != session_id {
            bail!("completion checkpoint stage/session identity mismatch");
        }
        Ok(())
    }

    fn observation_index(&self, nonce: &str) -> Option<usize> {
        self.observations
            .iter()
            .position(|item| item.evidence_nonce == nonce)
    }

    fn update_observation(
        &mut self,
        index: usize,
        evidence: &CompletionAttemptEvidence,
        fingerprint: Option<String>,
    ) -> Result<RecordOutcome> {
        let stored = &mut self.observations[index];
        if stored.identity_digest != evidence.identity_digest()
            || fingerprints_conflict(stored.fingerprint.as_ref(), fingerprint.as_ref())
        {
            self.conflict = true;
            return Ok(RecordOutcome::Conflict);
        }
        if stored.phase == evidence.phase
            && (stored.fingerprint == fingerprint
                || (fingerprint.is_none() && stored.fingerprint.is_some()))
        {
            return Ok(RecordOutcome::Duplicate);
        }
        stored.phase = evidence.phase;
        if stored.fingerprint.is_none() {
            stored.fingerprint = fingerprint;
        }
        replace_if_later(&mut stored.last_observed_at, &evidence.observed_at)?;
        Ok(RecordOutcome::PhaseUpdated)
    }

    fn apply_attempt(
        &mut self,
        evidence: &CompletionAttemptEvidence,
        blocker: Option<CompletionBlocker>,
    ) -> Result<()> {
        let replace_latest = match &self.latest {
            Some(latest) => evidence.observed_at_utc()? >= latest.observed_at_utc()?,
            None => true,
        };
        if replace_latest {
            self.latest = Some(evidence.clone());
            if blocker.is_some() {
                self.blocker = blocker;
            }
        }
        merge_bound(
            &mut self.first_observed_at,
            Some(&evidence.observed_at),
            true,
        )?;
        merge_bound(
            &mut self.last_observed_at,
            Some(&evidence.observed_at),
            false,
        )
    }

    fn merge_observation(&mut self, incoming: &NonceObservation) -> Result<()> {
        let Some(index) = self.observation_index(&incoming.evidence_nonce) else {
            if self.observations.len() < MAX_EVIDENCE_NONCES {
                self.observations.push(incoming.clone());
            } else {
                self.capacity_exhausted = true;
            }
            return Ok(());
        };
        let stored = &mut self.observations[index];
        if stored.identity_digest != incoming.identity_digest
            || fingerprints_conflict(stored.fingerprint.as_ref(), incoming.fingerprint.as_ref())
        {
            self.conflict = true;
            return Ok(());
        }
        let incoming_is_later =
            timestamp(&incoming.last_observed_at)? > timestamp(&stored.last_observed_at)?;
        let incoming_is_richer = stored.fingerprint.is_none() && incoming.fingerprint.is_some();
        let state_changed =
            incoming_is_richer || (incoming_is_later && stored.phase != incoming.phase);
        if state_changed {
            stored.phase = incoming.phase;
            stored.fingerprint.clone_from(&incoming.fingerprint);
            stored.attestation.clone_from(&incoming.attestation);
        }
        if incoming_is_later {
            stored
                .last_observed_at
                .clone_from(&incoming.last_observed_at);
        }
        Ok(())
    }

    fn merge_latest(&mut self, other: &CompletionCheckpoint) -> Result<()> {
        let replace = match (&self.latest, &other.latest) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(left), Some(right)) => right.observed_at_utc()? >= left.observed_at_utc()?,
        };
        if replace {
            self.latest.clone_from(&other.latest);
        }
        Ok(())
    }

    fn merge_blocker(&mut self, other: &CompletionCheckpoint) {
        let latest_blocker = self
            .latest
            .as_ref()
            .filter(|item| item.is_actionable())
            .and_then(|item| CompletionBlocker::from_attempt(item).ok());
        if let Some(blocker) = latest_blocker {
            self.blocker = Some(blocker);
        } else if self.blocker.is_none() {
            self.blocker.clone_from(&other.blocker);
        }
    }

    fn merge_accepted(&mut self, other: &CompletionCheckpoint) {
        match (&self.accepted, &other.accepted) {
            (Some(left), Some(right)) if left != right => self.conflict = true,
            (None, Some(receipt))
                if self
                    .observations
                    .iter()
                    .any(|item| item.evidence_nonce == receipt.evidence_nonce) =>
            {
                self.accepted.clone_from(&other.accepted);
            }
            (None, Some(_)) => self.conflict = true,
            _ => {}
        }
    }
}

fn new_observation(
    evidence: &CompletionAttemptEvidence,
    fingerprint: Option<String>,
) -> NonceObservation {
    NonceObservation {
        evidence_nonce: evidence.evidence_nonce.clone(),
        identity_digest: evidence.identity_digest(),
        fingerprint,
        phase: evidence.phase,
        first_observed_at: evidence.observed_at.clone(),
        last_observed_at: evidence.observed_at.clone(),
        attestation: None,
    }
}

fn fingerprints_conflict(left: Option<&String>, right: Option<&String>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left != right)
}

fn sort_observations(observations: &mut [NonceObservation]) -> Result<()> {
    let mut ordered = observations
        .iter()
        .cloned()
        .map(|item| {
            let key = (
                timestamp(&item.first_observed_at)?,
                item.evidence_nonce.clone(),
            );
            Ok((key, item))
        })
        .collect::<Result<Vec<_>>>()?;
    ordered.sort_by(|left, right| left.0.cmp(&right.0));
    for (slot, (_, item)) in observations.iter_mut().zip(ordered) {
        *slot = item;
    }
    Ok(())
}

fn timestamp(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn replace_if_later(current: &mut String, candidate: &str) -> Result<()> {
    if timestamp(candidate)? > timestamp(current)? {
        *current = candidate.to_string();
    }
    Ok(())
}

fn merge_bound(
    current: &mut Option<String>,
    candidate: Option<&String>,
    minimum: bool,
) -> Result<()> {
    let Some(candidate) = candidate else {
        return Ok(());
    };
    let replace = match current {
        Some(value) if minimum => timestamp(candidate)? < timestamp(value)?,
        Some(value) => timestamp(candidate)? > timestamp(value)?,
        None => true,
    };
    if replace {
        *current = Some(candidate.clone());
    }
    Ok(())
}
