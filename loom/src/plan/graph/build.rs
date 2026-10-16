//! Build and validate dependency graphs from plan definitions.
use super::{cycle, scheduling, ExecutionGraph, StageNode};
use crate::models::stage::StageStatus;
use crate::plan::schema::StageDefinition;
use anyhow::Result;
use std::collections::HashMap;

impl ExecutionGraph {
    /// Build an execution graph from stage definitions
    pub fn build(stages: Vec<StageDefinition>) -> Result<Self> {
        let mut nodes = HashMap::new();
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut parallel_groups: HashMap<String, Vec<String>> = HashMap::new();

        // First pass: create all nodes
        for stage in &stages {
            let node = node_from_definition(stage);
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

        populate_edges(&stages, &nodes, &mut edges)?;

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
}

fn node_from_definition(stage: &StageDefinition) -> StageNode {
    StageNode {
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
    }
}

fn populate_edges(
    stages: &[StageDefinition],
    nodes: &HashMap<String, StageNode>,
    edges: &mut HashMap<String, Vec<String>>,
) -> Result<()> {
    for stage in stages {
        for dependency in &stage.dependencies {
            anyhow::ensure!(
                nodes.contains_key(dependency),
                "Stage {} depends on missing stage {}",
                stage.id,
                dependency
            );
            edges
                .entry(dependency.clone())
                .or_default()
                .push(stage.id.clone());
        }
    }
    Ok(())
}
