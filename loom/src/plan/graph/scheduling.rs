//! Scheduling algorithms: topological sort, ready status updates

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet, VecDeque};

use super::nodes::{NodeStatus, StageNode};

/// Update which stages are ready (all deps satisfied AND merged).
///
/// A stage transitions from `WaitingForDeps` to `Queued` when:
/// - It has no dependencies, OR
/// - All dependencies have BOTH `status == Completed` AND `merged == true`
///
/// This ensures dependent stages can use the merge point (main) as their base,
/// which contains all dependency work.
///
/// Returns the list of stages that became ready.
pub fn update_ready_status(nodes: &mut HashMap<String, StageNode>) -> Vec<String> {
    let mut newly_ready = Vec::new();

    // Stages with no dependencies are immediately ready
    for node in nodes.values_mut() {
        if node.status == NodeStatus::WaitingForDeps && node.dependencies.is_empty() {
            node.status = NodeStatus::Queued;
            newly_ready.push(node.id.clone());
        }
    }

    // Collect stages that are completed AND merged - only these satisfy dependencies
    let completed_and_merged: HashSet<_> = nodes
        .values()
        .filter(|n| n.status == NodeStatus::Completed && n.merged)
        .map(|n| n.id.clone())
        .collect();

    for node in nodes.values_mut() {
        if node.status == NodeStatus::WaitingForDeps {
            // All deps must be completed AND merged
            let deps_satisfied = node
                .dependencies
                .iter()
                .all(|d| completed_and_merged.contains(d));
            if deps_satisfied {
                node.status = NodeStatus::Queued;
                newly_ready.push(node.id.clone());
            }
        }
    }

    newly_ready
}

/// Get a topologically sorted list of stages
pub fn topological_sort(
    nodes: &HashMap<String, StageNode>,
    edges: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();

    // Calculate in-degree for each node
    for node in nodes.values() {
        in_degree.entry(node.id.clone()).or_insert(0);
        for dep in &node.dependencies {
            *in_degree.entry(node.id.clone()).or_insert(0) += 1;
            in_degree.entry(dep.clone()).or_insert(0);
        }
    }

    // Start with nodes that have no dependencies
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(id, &degree)| degree == 0 && nodes.contains_key(*id))
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(node_id) = queue.pop_front() {
        result.push(node_id.clone());

        // Reduce in-degree of dependents
        if let Some(dependents) = edges.get(&node_id) {
            for dep in dependents {
                if let Some(degree) = in_degree.get_mut(dep) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }
    }

    if result.len() != nodes.len() {
        bail!("Cycle detected in graph");
    }

    Ok(result)
}
