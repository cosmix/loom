use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::handoff::completion::identity::expected_stage_commit;
use crate::handoff::{current_blocker, load_trusted_session_checkpoint, CompletionCheckpoint};
use crate::models::stage::{Stage, StageStatus};

use super::events::{CompletionEscalation, MonitorEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    session_id: String,
    fingerprint: String,
    repeat_count: u32,
    escalation: Option<CompletionEscalation>,
}

#[derive(Default)]
pub(crate) struct CompletionBlockerWatch {
    latch: HashMap<String, Observation>,
    read_errors: HashSet<(String, String)>,
}

pub(crate) struct BlockerScan {
    pub(crate) events: Vec<MonitorEvent>,
    pub(crate) owned_stage_ids: HashSet<String>,
}

impl CompletionBlockerWatch {
    pub(crate) fn scan(
        &mut self,
        stages: &[Stage],
        work_dir: &Path,
        repo_root: &Path,
        now: DateTime<Utc>,
    ) -> BlockerScan {
        self.scan_with(stages, work_dir, now, |stage| {
            expected_stage_commit(stage, repo_root).ok()
        })
    }

    fn scan_with<F>(
        &mut self,
        stages: &[Stage],
        work_dir: &Path,
        now: DateTime<Utc>,
        mut commit_for: F,
    ) -> BlockerScan
    where
        F: FnMut(&Stage) -> Option<String>,
    {
        let mut scan = BlockerScan::empty();
        let mut retained = HashSet::new();
        for stage in stages {
            self.scan_stage(
                stage,
                work_dir,
                now,
                &mut commit_for,
                &mut scan,
                &mut retained,
            );
        }
        self.latch.retain(|stage_id, _| retained.contains(stage_id));
        scan
    }

    fn scan_stage<F>(
        &mut self,
        stage: &Stage,
        work_dir: &Path,
        now: DateTime<Utc>,
        commit_for: &mut F,
        scan: &mut BlockerScan,
        retained: &mut HashSet<String>,
    ) where
        F: FnMut(&Stage) -> Option<String>,
    {
        let Some(session_id) = stage
            .session
            .as_deref()
            .filter(|_| stage.status == StageStatus::Executing)
        else {
            return;
        };
        let key = (stage.id.clone(), session_id.to_string());
        let checkpoint = match load_trusted_session_checkpoint(&stage.id, session_id, work_dir) {
            Ok(value) => {
                self.read_errors.remove(&key);
                value
            }
            Err(error) => {
                retained.insert(stage.id.clone());
                if self.read_errors.insert(key) {
                    tracing::warn!(
                        stage_id = %stage.id,
                        session_id,
                        error = %error,
                        "Completion checkpoint is unreadable"
                    );
                }
                return;
            }
        };
        let Some(observation) = checkpoint
            .as_ref()
            .and_then(|checkpoint| classify(checkpoint, stage, commit_for(stage).as_deref(), now))
        else {
            return;
        };
        retained.insert(stage.id.clone());
        scan.owned_stage_ids.insert(stage.id.clone());
        if self.latch.get(&stage.id) != Some(&observation) {
            scan.events.push(observation.event(&stage.id));
            self.latch.insert(stage.id.clone(), observation);
        }
    }
}

impl BlockerScan {
    fn empty() -> Self {
        Self {
            events: Vec::new(),
            owned_stage_ids: HashSet::new(),
        }
    }
}

impl Observation {
    fn event(&self, stage_id: &str) -> MonitorEvent {
        let fields = || {
            (
                stage_id.to_string(),
                self.session_id.clone(),
                self.fingerprint.clone(),
            )
        };
        match self.escalation {
            Some(escalation) => {
                let (stage_id, session_id, fingerprint) = fields();
                MonitorEvent::CompletionBlocked {
                    stage_id,
                    session_id,
                    fingerprint,
                    repeat_count: self.repeat_count,
                    escalation,
                }
            }
            None => {
                let (stage_id, session_id, fingerprint) = fields();
                MonitorEvent::CompletionPending {
                    stage_id,
                    session_id,
                    fingerprint,
                    repeat_count: self.repeat_count,
                }
            }
        }
    }
}

fn classify(
    checkpoint: &CompletionCheckpoint,
    stage: &Stage,
    current_commit: Option<&str>,
    now: DateTime<Utc>,
) -> Option<Observation> {
    if checkpoint.capacity_exhausted && checkpoint.accepted.is_none() {
        return Some(Observation {
            session_id: checkpoint.session_id.clone(),
            fingerprint: checkpoint
                .blocker
                .as_ref()
                .map(|blocker| blocker.fingerprint.clone())
                .unwrap_or_else(|| "capacity-exhausted".to_string()),
            repeat_count: checkpoint.repeat_count(),
            escalation: Some(CompletionEscalation::CapacityExhausted),
        });
    }
    let blocker = current_blocker(checkpoint, stage, current_commit)?;
    let repeat_count = checkpoint.repeat_count();
    let escalation = if repeat_count >= 2 {
        Some(CompletionEscalation::Repeated)
    } else if repeat_count == 1 && idle_budget_expired(checkpoint, stage, now) {
        Some(CompletionEscalation::IdleBudgetExpired)
    } else {
        None
    };
    Some(Observation {
        session_id: checkpoint.session_id.clone(),
        fingerprint: blocker.fingerprint.clone(),
        repeat_count,
        escalation,
    })
}

fn idle_budget_expired(
    checkpoint: &CompletionCheckpoint,
    stage: &Stage,
    now: DateTime<Utc>,
) -> bool {
    let Some(observed) = checkpoint
        .last_observed_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
    else {
        return false;
    };
    let elapsed = now
        .signed_duration_since(observed.with_timezone(&Utc))
        .num_seconds();
    u64::try_from(elapsed).is_ok_and(|seconds| seconds > stage.effective_subagent_timeout_secs())
}

pub(crate) fn filter_owned_hung_events(
    events: &mut Vec<MonitorEvent>,
    owned_stage_ids: &HashSet<String>,
) {
    events.retain(|event| {
        !matches!(event, MonitorEvent::SessionHung { stage_id: Some(stage_id), .. }
            if owned_stage_ids.contains(stage_id))
    });
}

#[cfg(test)]
#[path = "completion_blockers_tests.rs"]
mod tests;
