//! Execution graph for managing stage dependencies and execution order

mod cycle;
mod nodes;
mod scheduling;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::schema::StageDefinition;

pub use nodes::{NodeStatus, StageNode};

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
                status: NodeStatus::WaitingForDeps,
                description: stage.description.clone(),
                acceptance: stage.acceptance.clone(),
                setup: stage.setup.clone(),
                files: stage.files.clone(),
                auto_merge: stage.auto_merge,
                outputs: Vec::new(),
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
        scheduling::update_ready_status(&mut graph.nodes);

        Ok(graph)
    }

    /// Update which stages are ready (all deps satisfied)
    pub fn update_ready_status(&mut self) {
        scheduling::update_ready_status(&mut self.nodes);
    }

    /// Force update of ready status for all nodes (useful after recovery)
    pub fn refresh_ready_status(&mut self) {
        self.update_ready_status();
    }

    /// Get all stages that are ready to execute
    pub fn ready_stages(&self) -> Vec<&StageNode> {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Queued)
            .collect()
    }

    /// Get stages in a specific parallel group
    pub fn parallel_group(&self, name: &str) -> Vec<&StageNode> {
        self.parallel_groups
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all parallel group names
    pub fn parallel_group_names(&self) -> Vec<&String> {
        self.parallel_groups.keys().collect()
    }

    /// Mark a stage as executing
    pub fn mark_executing(&mut self, stage_id: &str) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        if node.status != NodeStatus::Queued {
            bail!(
                "Stage '{}' is not ready (status: {:?})",
                stage_id,
                node.status
            );
        }

        node.status = NodeStatus::Executing;
        Ok(())
    }

    /// Mark a stage as completed and update dependent stages
    pub fn mark_completed(&mut self, stage_id: &str) -> Result<Vec<String>> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        node.status = NodeStatus::Completed;

        // Get dependents that might now be ready
        let dependents = self.edges.get(stage_id).cloned().unwrap_or_default();

        // Update ready status for all nodes
        self.update_ready_status();

        // Return newly ready stages
        let newly_ready: Vec<String> = dependents
            .into_iter()
            .filter(|id| {
                self.nodes
                    .get(id)
                    .is_some_and(|n| n.status == NodeStatus::Queued)
            })
            .collect();

        Ok(newly_ready)
    }

    /// Mark a stage as blocked
    pub fn mark_blocked(&mut self, stage_id: &str) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;
        node.status = NodeStatus::Blocked;
        Ok(())
    }

    /// Mark a stage as skipped
    pub fn mark_skipped(&mut self, stage_id: &str) -> Result<()> {
        let node = self
            .nodes
            .get_mut(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;
        node.status = NodeStatus::Skipped;
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
                if dep_node.status != NodeStatus::Completed {
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
        node.status = NodeStatus::Queued;
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

    /// Check if all stages are completed or skipped
    pub fn is_complete(&self) -> bool {
        self.nodes
            .values()
            .all(|n| n.status == NodeStatus::Completed || n.status == NodeStatus::Skipped)
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
}
