//! Execution graph for managing stage dependencies and execution order

mod cycle;
mod loader;
mod nodes;
mod scheduling;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::stage::StageStatus;

use super::schema::StageDefinition;

pub use loader::build_execution_graph;
pub use nodes::StageNode;

/// Execution graph representing stages and their dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionGraph {
    /// Map from stage ID to stage node
    nodes: HashMap<String, StageNode>,
    /// Adjacency list: stage_id -> list of stages that depend on it
    edges: HashMap<String, Vec<String>>,
    /// Map from parallel group name to stage IDs
    parallel_groups: HashMap<String, Vec<String>>,
}

impl ExecutionGraph {
    /// Build an execution graph from stage definitions
    pub fn build(stages: Vec<StageDefinition>) -> Result<Self> {
        let mut nodes = HashMap::new();
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut parallel_groups: HashMap<String, Vec<String>> = HashMap::new();

        // First pass: create all nodes
        for stage in &stages {
            let node = StageNode {
                id: stage.id.clone(),
                name: stage.name.clone(),
                dependencies: stage.dependencies.clone(),
                parallel_group: stage.parallel_group.clone(),
                status: StageStatus::WaitingForDeps,
                description: stage.description.clone(),
                acceptance: stage.acceptance.clone(),
                setup: stage.setup.clone(),
                files: stage.files.clone(),
                auto_merge: stage.auto_merge,
                outputs: Vec::new(),
                merged: false,
            };
            nodes.insert(stage.id.clone(), node);

            // Add to parallel group if specified
            if let Some(group) = &stage.parallel_group {
                parallel_groups
                    .entry(group.clone())
                    .or_default()
                    .push(stage.id.clone());
            }

            // Initialize edges entry
            edges.entry(stage.id.clone()).or_default();
        }

        // Second pass: build edges (reverse dependencies)
        for stage in &stages {
            for dep in &stage.dependencies {
                edges.entry(dep.clone()).or_default().push(stage.id.clone());
            }
        }

        let graph = Self {
            nodes,
            edges,
            parallel_groups,
        };

        // Check for cycles
        cycle::detect_cycles(&graph.nodes)?;

        // Update initial ready status
        let mut graph = graph;
        let _ = scheduling::update_ready_status(&mut graph.nodes);

        Ok(graph)
    }

    /// Update which stages are ready (all deps satisfied and merged).
    ///
    /// # Returns
    ///
    /// A vector of stage IDs that transitioned from `WaitingForDeps` to `Queued`.
    /// Callers should persist these stages to disk to keep the file state synchronized
    /// with the graph state.
    ///
    /// # Graph/File State Synchronization
    ///
    /// The execution graph tracks scheduling decisions in memory, but stage files on
    /// disk are the persistent source of truth. After calling this method, callers
    /// should write the returned stage IDs back to their files (e.g., via
    /// `Orchestrator::sync_specific_stages_to_files()`).
    pub fn update_ready_status(&mut self) -> Vec<String> {
        scheduling::update_ready_status(&mut self.nodes)
    }

    /// Force update of ready status for all nodes (useful after recovery).
    ///
    /// Returns the list of stages that became ready.
    pub fn refresh_ready_status(&mut self) -> Vec<String> {
        self.update_ready_status()
    }

    /// Get all stages that are ready to execute
    pub fn ready_stages(&self) -> Vec<&StageNode> {
        self.nodes
            .values()
            .filter(|n| n.status == StageStatus::Queued)
            .collect()
    }

    /// Get stages in a specific parallel group
    #[cfg(test)]
    pub fn parallel_group(&self, name: &str) -> Vec<&StageNode> {
        self.parallel_groups
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    /// Mark a stage as executing.
    ///
    /// Validates that the stage is currently in `Queued` status.
    pub fn mark_executing(&mut self, stage_id: &str) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        if node.status != StageStatus::Queued {
            bail!(
                "Stage '{}' is not ready (status: {:?})",
                stage_id,
                node.status
            );
        }

        node.status = StageStatus::Executing;
        Ok(())
    }

    /// Mark a stage as completed and update dependent stages.
    ///
    /// # Returns
    ///
    /// A vector of stage IDs that became ready (transitioned to `Queued`) as a result
    /// of this stage completing. Note that stages only become ready when ALL their
    /// dependencies are both completed AND merged.
    pub fn mark_completed(&mut self, stage_id: &str) -> Result<Vec<String>> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        node.status = StageStatus::Completed;

        // Update ready status for all nodes - this returns stages that became ready
        let newly_ready = self.update_ready_status();

        Ok(newly_ready)
    }

    /// Set a stage to an arbitrary status without validation.
    ///
    /// Use this for simple status updates where no side effects (like updating
    /// dependent stages) are needed. For statuses that have special semantics,
    /// prefer the dedicated methods:
    /// - `mark_executing()` - validates current status is Queued
    /// - `mark_completed()` - triggers dependent stage readiness check
    /// - `mark_merged()` - validates Completed status and triggers readiness check
    /// - `mark_queued()` - validates all dependencies are completed
    pub fn mark_status(&mut self, stage_id: &str, status: StageStatus) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;
        node.status = status;
        Ok(())
    }

    /// Mark a stage as queued for execution.
    ///
    /// Used to reset orphaned/blocked stages back to queued state.
    /// Only succeeds if all dependencies are completed.
    pub fn mark_queued(&mut self, stage_id: &str) -> Result<()> {
        // First check dependencies
        let deps = {
            let node = self
                .nodes
                .get(stage_id)
                .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;
            node.dependencies.clone()
        };

        // Verify all dependencies are completed
        for dep in &deps {
            if let Some(dep_node) = self.nodes.get(dep) {
                if dep_node.status != StageStatus::Completed {
                    bail!(
                        "Cannot mark '{}' as ready: dependency '{}' is {:?}",
                        stage_id,
                        dep,
                        dep_node.status
                    );
                }
            }
        }

        // All deps satisfied, mark as ready
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;
        node.status = StageStatus::Queued;
        Ok(())
    }

    /// Get a topologically sorted list of stages
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        scheduling::topological_sort(&self.nodes, &self.edges)
    }

    /// Get a specific node by ID
    pub fn get_node(&self, stage_id: &str) -> Option<&StageNode> {
        self.nodes.get(stage_id)
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<&StageNode> {
        self.nodes.values().collect()
    }

    /// Check if all stages are completed or skipped.
    ///
    /// Note: Stages with failures (CompletedWithFailures, MergeConflict, MergeBlocked)
    /// are not considered complete since they may need retry or manual intervention.
    pub fn is_complete(&self) -> bool {
        self.nodes
            .values()
            .all(|n| n.status == StageStatus::Completed || n.status == StageStatus::Skipped)
    }

    /// Get all leaf stages (stages with no dependents).
    ///
    /// Leaf stages are stages that no other stage depends on. These are typically
    /// the final stages in an execution plan that produce the ultimate outputs.
    /// In the DAG representation, these are nodes with no outgoing edges
    /// (no other nodes list them as dependencies).
    #[cfg(test)]
    pub fn leaf_stages(&self) -> Vec<&str> {
        self.nodes
            .keys()
            .filter(|id| self.edges.get(*id).is_none_or(|deps| deps.is_empty()))
            .map(|s| s.as_str())
            .collect()
    }

    /// Update the outputs for a stage node.
    ///
    /// This is called when syncing stage state from disk to the graph,
    /// to ensure dependency outputs are available for signal generation.
    pub fn set_node_outputs(
        &mut self,
        stage_id: &str,
        outputs: Vec<crate::models::stage::StageOutput>,
    ) {
        if let Some(node) = self.nodes.get_mut(stage_id) {
            node.outputs = outputs;
        }
    }

    /// Mark a stage as merged to the merge point.
    ///
    /// This is called after progressive merge verifies and merges a completed stage.
    /// After marking as merged, dependent stages may become ready for scheduling.
    ///
    /// # Returns
    ///
    /// A vector of stage IDs that became ready (transitioned to `Queued`) as a result
    /// of this stage being merged. Since stages require dependencies to be both completed
    /// AND merged, this merge operation may unblock waiting dependent stages.
    pub fn mark_merged(&mut self, stage_id: &str) -> Result<Vec<String>> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        if node.status != StageStatus::Completed {
            bail!(
                "Cannot mark '{}' as merged: status is {:?}, expected Completed",
                stage_id,
                node.status
            );
        }

        node.merged = true;

        // Update ready status for all nodes - this returns stages that became ready
        let newly_ready = self.update_ready_status();

        Ok(newly_ready)
    }

    /// Set the merged status for a stage node (used during state sync from disk).
    pub fn set_node_merged(&mut self, stage_id: &str, merged: bool) {
        if let Some(node) = self.nodes.get_mut(stage_id) {
            node.merged = merged;
        }
    }
}
