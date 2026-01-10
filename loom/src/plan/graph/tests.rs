//! Tests for the execution graph

use super::*;
use crate::plan::schema::StageDefinition;

fn make_stage(id: &str, deps: Vec<&str>, group: Option<&str>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        dependencies: deps.into_iter().map(String::from).collect(),
        parallel_group: group.map(String::from),
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
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
    assert_eq!(graph.get_node("b").unwrap().status, NodeStatus::Queued);
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

#[test]
fn test_skipped_does_not_satisfy_deps() {
    let stages = vec![
        make_stage("a", vec![], None),
        make_stage("b", vec!["a"], None),
    ];

    let mut graph = ExecutionGraph::build(stages).unwrap();

    // Mark stage 'a' as skipped
    graph.mark_skipped("a").unwrap();
    assert_eq!(graph.get_node("a").unwrap().status, NodeStatus::Skipped);

    // Update ready status - stage 'b' should remain WaitingForDeps
    graph.update_ready_status();
    assert_eq!(
        graph.get_node("b").unwrap().status,
        NodeStatus::WaitingForDeps
    );

    // Verify that 'b' is not in the ready stages list
    let ready = graph.ready_stages();
    assert!(ready.is_empty());
}

#[test]
fn test_is_complete_with_skipped() {
    let stages = vec![make_stage("a", vec![], None), make_stage("b", vec![], None)];

    let mut graph = ExecutionGraph::build(stages).unwrap();

    // Complete one stage and skip another
    graph.mark_executing("a").unwrap();
    graph.mark_completed("a").unwrap();
    graph.mark_skipped("b").unwrap();

    // Graph should be considered complete
    assert!(graph.is_complete());
}

#[test]
fn test_leaf_stages() {
    // Linear chain: a -> b -> c
    // Leaf stage is "c" (nothing depends on it)
    let stages = vec![
        make_stage("a", vec![], None),
        make_stage("b", vec!["a"], None),
        make_stage("c", vec!["b"], None),
    ];

    let graph = ExecutionGraph::build(stages).unwrap();
    let leaves = graph.leaf_stages();

    assert_eq!(leaves.len(), 1);
    assert!(leaves.contains(&"c"));
}

#[test]
fn test_leaf_stages_diamond() {
    // Diamond pattern: a -> b, a -> c, b -> d, c -> d
    // Leaf stage is "d" (nothing depends on it)
    let stages = vec![
        make_stage("a", vec![], None),
        make_stage("b", vec!["a"], None),
        make_stage("c", vec!["a"], None),
        make_stage("d", vec!["b", "c"], None),
    ];

    let graph = ExecutionGraph::build(stages).unwrap();
    let leaves = graph.leaf_stages();

    assert_eq!(leaves.len(), 1);
    assert!(leaves.contains(&"d"));
}

#[test]
fn test_leaf_stages_multiple_leaves() {
    // Multiple leaves: a -> b, a -> c (both b and c are leaves)
    let stages = vec![
        make_stage("a", vec![], None),
        make_stage("b", vec!["a"], None),
        make_stage("c", vec!["a"], None),
    ];

    let graph = ExecutionGraph::build(stages).unwrap();
    let leaves = graph.leaf_stages();

    assert_eq!(leaves.len(), 2);
    assert!(leaves.contains(&"b"));
    assert!(leaves.contains(&"c"));
}

#[test]
fn test_leaf_stages_all_independent() {
    // All independent stages (no dependencies) - all are leaves
    let stages = vec![
        make_stage("a", vec![], None),
        make_stage("b", vec![], None),
        make_stage("c", vec![], None),
    ];

    let graph = ExecutionGraph::build(stages).unwrap();
    let leaves = graph.leaf_stages();

    assert_eq!(leaves.len(), 3);
    assert!(leaves.contains(&"a"));
    assert!(leaves.contains(&"b"));
    assert!(leaves.contains(&"c"));
}
