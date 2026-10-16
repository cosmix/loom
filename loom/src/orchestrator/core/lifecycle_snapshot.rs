//! Authoritative stage observations and cause-aware lifecycle decisions.
//! A snapshot holds the stages directory lock for the entire read; graph views
//! are derived from it, while mutations revalidate under the same writer lock.
use super::{
    recovery::{scan_stage_paths, StageScanCounter},
    Orchestrator,
};
use crate::models::stage::{Stage, StageStatus};
use anyhow::{ensure, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum BlockCause {
    Intentional,
    Failure,
}

pub(super) fn block_cause(stage: &Stage) -> Option<BlockCause> {
    (stage.status == StageStatus::Blocked).then(|| {
        if stage.failure_info.is_some() {
            BlockCause::Failure
        } else {
            BlockCause::Intentional
        }
    })
}

pub(super) struct StageSnapshot {
    pub(super) records: HashMap<String, (Stage, PathBuf)>,
}

impl Orchestrator {
    pub(super) fn stage_snapshot(&self) -> Result<StageSnapshot> {
        StageSnapshot::load(&self.config.work_dir, &self.graph)
    }
}

impl StageSnapshot {
    pub(super) fn load(work_dir: &Path, graph: &crate::plan::ExecutionGraph) -> Result<Self> {
        let dir = work_dir.join("stages");
        let records = crate::fs::locking::locked_dir_update(&dir, || {
            let mut records = HashMap::new();
            for path in scan_stage_paths(&dir, &mut StageScanCounter::default())? {
                let content = std::fs::read_to_string(&path)?;
                let stage: Stage = crate::verify::transitions::parse_stage_from_markdown(&content)
                    .with_context(|| format!("Invalid stage record {}", path.display()))?;
                crate::validation::validate_id(&stage.id)?;
                ensure!(
                    !records.contains_key(&stage.id),
                    "Duplicate stage ID: {}",
                    stage.id
                );
                records.insert(stage.id.clone(), (stage, path));
            }
            Ok(records)
        })?;
        for node in graph.all_nodes() {
            ensure!(
                records.contains_key(&node.id),
                "Missing persisted stage: {}",
                node.id
            );
        }
        for (stage, _) in records.values() {
            ensure!(
                graph.get_node(&stage.id).is_some(),
                "Unexpected persisted stage: {}",
                stage.id
            );
            for dependency in &stage.dependencies {
                ensure!(
                    records.contains_key(dependency),
                    "Stage {} has missing dependency {}",
                    stage.id,
                    dependency
                );
            }
        }
        Ok(StageSnapshot { records })
    }
}

impl StageSnapshot {
    /// Pure candidate selection; capacity is consumed only after launch/adoption.
    pub(super) fn ready_ids(&self) -> Vec<String> {
        let stages: Vec<&Stage> = self.records.values().map(|(stage, _)| stage).collect();
        let levels = crate::plan::graph::levels::compute_all_levels(
            &stages,
            |stage| stage.id.as_str(),
            |stage| stage.dependencies.as_slice(),
        );
        let mut ready: Vec<String> = stages
            .iter()
            .filter(|stage| {
                matches!(
                    stage.status,
                    StageStatus::Queued | StageStatus::WaitingForDeps
                ) && stage.dependencies.iter().all(|id| {
                    self.records
                        .get(id)
                        .is_some_and(|(dep, _)| dep.status == StageStatus::Completed && dep.merged)
                })
            })
            .map(|stage| stage.id.clone())
            .collect();
        ready.sort_by_key(|id| (levels.get(id).copied().unwrap_or(usize::MAX), id.clone()));
        ready
    }
}
