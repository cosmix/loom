//! Cycle detection algorithms for the execution graph

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

use super::nodes::StageNode;

/// Detect circular dependencies in the graph using DFS
pub fn detect_cycles(nodes: &HashMap<String, StageNode>) -> Result<()> {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut cycle_path = Vec::new();

    for node_id in nodes.keys() {
        if !visited.contains(node_id) {
            if let Some(cycle) =
                dfs_detect_cycle(nodes, node_id, &mut visited, &mut rec_stack, &mut cycle_path)
            {
                bail!("Circular dependency detected: {}", cycle.join(" -> "));
            }
        }
    }

    Ok(())
}

/// DFS helper for cycle detection
fn dfs_detect_cycle(
    nodes: &HashMap<String, StageNode>,
    node_id: &str,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    visited.insert(node_id.to_string());
    rec_stack.insert(node_id.to_string());
    path.push(node_id.to_string());

    // Get dependencies for this node
    if let Some(node) = nodes.get(node_id) {
        for dep in &node.dependencies {
            if !visited.contains(dep) {
                if let Some(cycle) = dfs_detect_cycle(nodes, dep, visited, rec_stack, path) {
                    return Some(cycle);
                }
            } else if rec_stack.contains(dep) {
                // Found cycle - build the cycle path
                let mut cycle = vec![dep.clone()];
                for p in path.iter().rev() {
                    cycle.push(p.clone());
                    if p == dep {
                        break;
                    }
                }
                cycle.reverse();
                return Some(cycle);
            }
        }
    }

    path.pop();
    rec_stack.remove(node_id);
    None
}
