//! The per-invocation projection the status dashboards list.

use chrono::{DateTime, Utc};

use super::StageRecords;
use crate::native_worker::model::{NativeEvent, Outcome, Phase};

/// One invocation as the dashboards show it, whether or not a Codex thread started.
#[derive(Debug, PartialEq, Eq)]
pub struct NativeJob {
    pub unit_id: String,
    pub invocation_id: String,
    pub model: String,
    pub effort: String,
    pub phase: Phase,
    pub outcome: Option<Outcome>,
    pub outcome_reason: Option<String>,
    pub thread_id: Option<String>,
    pub started_at: DateTime<Utc>,
    /// The `at` of the `Terminal` record; `None` while the unit runs.
    pub ended_at: Option<DateTime<Utc>>,
    pub deadline_secs: u64,
}

/// Every invocation of `records`, oldest first.
pub fn jobs_in(records: &StageRecords) -> Vec<NativeJob> {
    let mut jobs: Vec<NativeJob> = records
        .0
        .iter()
        .map(|(unit, events)| {
            let ended_at = events.iter().find_map(|record| match record.event {
                NativeEvent::Terminal { .. } => Some(record.at),
                _ => None,
            });
            NativeJob {
                unit_id: unit.manifest.unit_id.clone(),
                invocation_id: unit.manifest.invocation_id.clone(),
                model: unit.manifest.model.clone(),
                effort: unit.manifest.effort.clone(),
                phase: unit.phase,
                outcome: unit.outcome,
                outcome_reason: unit.outcome_reason.clone(),
                thread_id: unit.thread_id.clone(),
                started_at: unit.manifest.created_at,
                ended_at,
                deadline_secs: unit.manifest.deadline_secs,
            }
        })
        .collect();
    jobs.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then_with(|| a.invocation_id.cmp(&b.invocation_id))
    });
    jobs
}
