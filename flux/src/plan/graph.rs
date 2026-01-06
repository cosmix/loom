//! Execution graph for managing stage dependencies and execution order

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use super::schema::StageDefinition;

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

/// A node in the execution graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageNode {
    pub id: String,
    pub name: String,
    pub dependencies: Vec<String>,
    pub parallel_group: Option<String>,
    pub status: NodeStatus,
}

/// Status of a node in the graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Ready,
    Executing,
    Completed,
    Blocked,
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
                status: NodeStatus::Pending,
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
        graph.detect_cycles()?;

        // Update initial ready status
        let mut graph = graph;
        graph.update_ready_status();

        Ok(graph)
    }

    /// Detect circular dependencies using DFS
    fn detect_cycles(&self) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycle_path = Vec::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                if let Some(cycle) =
                    self.dfs_detect_cycle(node_id, &mut visited, &mut rec_stack, &mut cycle_path)
                {
                    bail!("Circular dependency detected: {}", cycle.join(" -> "));
                }
            }
        }

        Ok(())
    }

    /// DFS helper for cycle detection
    fn dfs_detect_cycle(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        // Get dependencies for this node
        if let Some(node) = self.nodes.get(node_id) {
            for dep in &node.dependencies {
                if !visited.contains(dep) {
                    if let Some(cycle) = self.dfs_detect_cycle(dep, visited, rec_stack, path) {
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

    /// Update which stages are ready (all deps satisfied)
    fn update_ready_status(&mut self) {
        for node in self.nodes.values_mut() {
            if node.status == NodeStatus::Pending && node.dependencies.is_empty() {
                node.status = NodeStatus::Ready;
            }
        }

        // Also check if dependencies are completed
        let completed: HashSet<_> = self
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Completed)
            .map(|n| n.id.clone())
            .collect();

        for node in self.nodes.values_mut() {
            if node.status == NodeStatus::Pending {
                let deps_satisfied = node.dependencies.iter().all(|d| completed.contains(d));
                if deps_satisfied {
                    node.status = NodeStatus::Ready;
                }
            }
        }
    }

    /// Get all stages that are ready to execute
    pub fn ready_stages(&self) -> Vec<&StageNode> {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Ready)
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

        if node.status != NodeStatus::Ready {
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
                    .is_some_and(|n| n.status == NodeStatus::Ready)
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

    /// Get a topologically sorted list of stages
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Calculate in-degree for each node
        for node in self.nodes.values() {
            in_degree.entry(node.id.clone()).or_insert(0);
            for dep in &node.dependencies {
                *in_degree.entry(node.id.clone()).or_insert(0) += 1;
                in_degree.entry(dep.clone()).or_insert(0);
            }
        }

        // Start with nodes that have no dependencies
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(id, &degree)| degree == 0 && self.nodes.contains_key(*id))
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            // Reduce in-degree of dependents
            if let Some(dependents) = self.edges.get(&node_id) {
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

        if result.len() != self.nodes.len() {
            bail!("Cycle detected in graph");
        }

        Ok(result)
    }

    /// Get a specific node by ID
    pub fn get_node(&self, stage_id: &str) -> Option<&StageNode> {
        self.nodes.get(stage_id)
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<&StageNode> {
        self.nodes.values().collect()
    }

    /// Check if all stages are completed
    pub fn is_complete(&self) -> bool {
        self.nodes
            .values()
            .all(|n| n.status == NodeStatus::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stage(id: &str, deps: Vec<&str>, group: Option<&str>) -> StageDefinition {
        StageDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            dependencies: deps.into_iter().map(String::from).collect(),
            parallel_group: group.map(String::from),
            acceptance: vec![],
            files: vec![],
        }
    }

    #[test]
    fn test_build_simple_graph() {
        let stages = vec![
            make_stage("a", vec![], None),
            make_stage("b", vec!["a"], None),
            make_stage("c", vec!["b"], None),
        ];

        let graph = ExecutionGraph::build(stages).unwrap();

        assert_eq!(graph.nodes.len(), 3);

        let ready = graph.ready_stages();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a");
    }

    #[test]
    fn test_parallel_groups() {
        let stages = vec![
            make_stage("a", vec![], None),
            make_stage("b", vec!["a"], Some("parallel")),
            make_stage("c", vec!["a"], Some("parallel")),
            make_stage("d", vec!["b", "c"], None),
        ];

        let graph = ExecutionGraph::build(stages).unwrap();

        let parallel = graph.parallel_group("parallel");
        assert_eq!(parallel.len(), 2);
    }

    #[test]
    fn test_detect_cycle() {
        let stages = vec![
            make_stage("a", vec!["c"], None),
            make_stage("b", vec!["a"], None),
            make_stage("c", vec!["b"], None),
        ];

        let result = ExecutionGraph::build(stages);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circular"));
    }

    #[test]
    fn test_mark_completed() {
        let stages = vec![
            make_stage("a", vec![], None),
            make_stage("b", vec!["a"], None),
        ];

        let mut graph = ExecutionGraph::build(stages).unwrap();

        graph.mark_executing("a").unwrap();
        let newly_ready = graph.mark_completed("a").unwrap();

        assert_eq!(newly_ready, vec!["b"]);
        assert_eq!(graph.get_node("b").unwrap().status, NodeStatus::Ready);
    }

    #[test]
    fn test_topological_sort() {
        let stages = vec![
            make_stage("c", vec!["a", "b"], None),
            make_stage("a", vec![], None),
            make_stage("b", vec!["a"], None),
        ];

        let graph = ExecutionGraph::build(stages).unwrap();
        let sorted = graph.topological_sort().unwrap();

        // a must come before b and c, b must come before c
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();

        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_c);
    }
}
