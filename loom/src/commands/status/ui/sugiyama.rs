//! Sugiyama layout algorithm for DAG visualization.
//!
//! Implements the classic Sugiyama framework for layered graph drawing:
//! 1. Layer assignment (computed externally from dependencies)
//! 2. Node ordering via barycenter heuristic
//! 3. Coordinate assignment with spacing constraints
//! 4. Orthogonal edge routing

use std::collections::{HashMap, HashSet};

use crate::models::stage::Stage;

/// Position and dimensions of a node in the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePosition {
    /// Horizontal position (left edge)
    pub x: f64,
    /// Vertical position (top edge)
    pub y: f64,
    /// Node width
    pub width: f64,
    /// Node height
    pub height: f64,
}

impl NodePosition {
    /// Create a new node position.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the center x coordinate.
    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    /// Get the center y coordinate.
    pub fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }

    /// Get the bottom center point (for outgoing edges).
    pub fn bottom_center(&self) -> (f64, f64) {
        (self.center_x(), self.y + self.height)
    }

    /// Get the top center point (for incoming edges).
    pub fn top_center(&self) -> (f64, f64) {
        (self.center_x(), self.y)
    }
}

/// A line segment in an edge path.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl LineSegment {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }
}

/// Path of an edge from source to target.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePath {
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Line segments forming the path
    pub segments: Vec<LineSegment>,
}

impl EdgePath {
    pub fn new(source: String, target: String, segments: Vec<LineSegment>) -> Self {
        Self {
            source,
            target,
            segments,
        }
    }
}

/// Bounding box of the entire layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl LayoutBounds {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

/// Complete layout result with node positions and edge paths.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    /// Node positions keyed by stage ID
    nodes: HashMap<String, NodePosition>,
    /// Edge paths from source to target
    edges: Vec<EdgePath>,
    /// Bounding box of the layout
    bounds: LayoutBounds,
}

impl LayoutResult {
    /// Create a new layout result.
    pub fn new(
        nodes: HashMap<String, NodePosition>,
        edges: Vec<EdgePath>,
        bounds: LayoutBounds,
    ) -> Self {
        Self {
            nodes,
            edges,
            bounds,
        }
    }

    /// Get position of a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&NodePosition> {
        self.nodes.get(id)
    }

    /// Get all node positions.
    pub fn nodes(&self) -> &HashMap<String, NodePosition> {
        &self.nodes
    }

    /// Iterate over all edges.
    pub fn edges(&self) -> &[EdgePath] {
        &self.edges
    }

    /// Get the layout bounds.
    pub fn bounds(&self) -> &LayoutBounds {
        &self.bounds
    }

    /// Get number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Check if layout is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Configuration for the layout algorithm.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Minimum horizontal spacing between nodes
    pub horizontal_spacing: f64,
    /// Minimum vertical spacing between layers
    pub vertical_spacing: f64,
    /// Default node width
    pub node_width: f64,
    /// Default node height
    pub node_height: f64,
    /// Number of barycenter iterations
    pub barycenter_iterations: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            horizontal_spacing: 20.0,
            vertical_spacing: 40.0,
            node_width: 80.0,
            node_height: 30.0,
            barycenter_iterations: 4,
        }
    }
}

/// Compute the layer (depth) for each stage based on dependencies.
fn compute_layers(stages: &[Stage]) -> HashMap<String, usize> {
    let mut layers: HashMap<String, usize> = HashMap::new();
    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();

    fn get_layer(
        stage_id: &str,
        stage_map: &HashMap<&str, &Stage>,
        layers: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> usize {
        if let Some(&layer) = layers.get(stage_id) {
            return layer;
        }

        if visiting.contains(stage_id) {
            return 0;
        }
        visiting.insert(stage_id.to_string());

        let stage = match stage_map.get(stage_id) {
            Some(s) => s,
            None => {
                visiting.remove(stage_id);
                return 0;
            }
        };

        let layer = if stage.dependencies.is_empty() {
            0
        } else {
            stage
                .dependencies
                .iter()
                .map(|dep| get_layer(dep, stage_map, layers, visiting) + 1)
                .max()
                .unwrap_or(0)
        };

        visiting.remove(stage_id);
        layers.insert(stage_id.to_string(), layer);
        layer
    }

    let mut visiting = HashSet::new();
    for stage in stages {
        get_layer(&stage.id, &stage_map, &mut layers, &mut visiting);
    }

    layers
}

/// Group stages by their layer.
fn group_by_layer(stages: &[Stage], layers: &HashMap<String, usize>) -> Vec<Vec<String>> {
    let max_layer = layers.values().copied().max().unwrap_or(0);
    let mut layer_groups: Vec<Vec<String>> = vec![Vec::new(); max_layer + 1];

    for stage in stages {
        if let Some(&layer) = layers.get(&stage.id) {
            layer_groups[layer].push(stage.id.clone());
        }
    }

    layer_groups
}

/// Calculate the maximum width needed for each layer.
fn calculate_layer_widths(layer_groups: &[Vec<String>], config: &LayoutConfig) -> Vec<f64> {
    layer_groups
        .iter()
        .map(|nodes| {
            if nodes.is_empty() {
                0.0
            } else {
                let nodes_width = nodes.len() as f64 * config.node_width;
                let spacing = (nodes.len().saturating_sub(1)) as f64 * config.horizontal_spacing;
                nodes_width + spacing
            }
        })
        .collect()
}

/// Compute barycenter (average x-position of neighbors) for a node.
fn compute_barycenter(
    node_id: &str,
    neighbor_positions: &HashMap<String, f64>,
    stages: &[Stage],
    is_downward: bool,
) -> Option<f64> {
    let stage = stages.iter().find(|s| s.id == node_id)?;

    let neighbors: Vec<f64> = if is_downward {
        stage
            .dependencies
            .iter()
            .filter_map(|dep| neighbor_positions.get(dep))
            .copied()
            .collect()
    } else {
        stages
            .iter()
            .filter(|s| s.dependencies.contains(&node_id.to_string()))
            .filter_map(|s| neighbor_positions.get(&s.id))
            .copied()
            .collect()
    };

    if neighbors.is_empty() {
        None
    } else {
        Some(neighbors.iter().sum::<f64>() / neighbors.len() as f64)
    }
}

/// Order nodes within each layer using barycenter heuristic.
fn order_by_barycenter(
    layer_groups: &mut [Vec<String>],
    stages: &[Stage],
    iterations: usize,
) -> HashMap<String, f64> {
    let mut positions: HashMap<String, f64> = HashMap::new();

    for (layer_idx, layer) in layer_groups.iter().enumerate() {
        for (order, node_id) in layer.iter().enumerate() {
            positions.insert(node_id.clone(), order as f64);
        }
        let _ = layer_idx;
    }

    for iteration in 0..iterations {
        let downward = iteration % 2 == 0;

        let layer_range: Vec<usize> = if downward {
            (1..layer_groups.len()).collect()
        } else {
            (0..layer_groups.len().saturating_sub(1)).rev().collect()
        };

        for layer_idx in layer_range {
            let mut node_barycenters: Vec<(String, f64)> = Vec::new();

            for node_id in &layer_groups[layer_idx] {
                let barycenter =
                    compute_barycenter(node_id, &positions, stages, downward).unwrap_or_else(|| {
                        positions.get(node_id).copied().unwrap_or(0.0)
                    });
                node_barycenters.push((node_id.clone(), barycenter));
            }

            node_barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            layer_groups[layer_idx] = node_barycenters.iter().map(|(id, _)| id.clone()).collect();

            for (order, (node_id, _)) in node_barycenters.iter().enumerate() {
                positions.insert(node_id.clone(), order as f64);
            }
        }
    }

    positions
}

/// Assign final x-coordinates to nodes with spacing.
fn assign_x_coordinates(
    layer_groups: &[Vec<String>],
    config: &LayoutConfig,
) -> HashMap<String, f64> {
    let layer_widths = calculate_layer_widths(layer_groups, config);
    let max_width = layer_widths.iter().copied().fold(0.0, f64::max);

    let mut x_coords: HashMap<String, f64> = HashMap::new();

    for layer in layer_groups {
        let layer_width = if layer.is_empty() {
            0.0
        } else {
            let nodes_width = layer.len() as f64 * config.node_width;
            let spacing = (layer.len().saturating_sub(1)) as f64 * config.horizontal_spacing;
            nodes_width + spacing
        };

        let start_x = (max_width - layer_width) / 2.0;

        for (idx, node_id) in layer.iter().enumerate() {
            let x = start_x + idx as f64 * (config.node_width + config.horizontal_spacing);
            x_coords.insert(node_id.clone(), x);
        }
    }

    x_coords
}

/// Create node positions from computed coordinates.
fn create_node_positions(
    stages: &[Stage],
    layers: &HashMap<String, usize>,
    x_coords: &HashMap<String, f64>,
    config: &LayoutConfig,
) -> HashMap<String, NodePosition> {
    let mut positions: HashMap<String, NodePosition> = HashMap::new();

    for stage in stages {
        let layer = layers.get(&stage.id).copied().unwrap_or(0);
        let x = x_coords.get(&stage.id).copied().unwrap_or(0.0);
        let y = layer as f64 * (config.node_height + config.vertical_spacing);

        positions.insert(
            stage.id.clone(),
            NodePosition::new(x, y, config.node_width, config.node_height),
        );
    }

    positions
}

/// Edge bundle entry: (source_id, target_id, source_x, target_x)
type EdgeBundleEntry = (String, String, f64, f64);

/// Route edges with orthogonal paths.
fn route_edges(stages: &[Stage], positions: &HashMap<String, NodePosition>) -> Vec<EdgePath> {
    let mut edges: Vec<EdgePath> = Vec::new();
    let mut edge_bundles: HashMap<(usize, usize), Vec<EdgeBundleEntry>> = HashMap::new();

    for stage in stages {
        let target_pos = match positions.get(&stage.id) {
            Some(p) => p,
            None => continue,
        };

        for dep in &stage.dependencies {
            let source_pos = match positions.get(dep) {
                Some(p) => p,
                None => continue,
            };

            let (src_x, src_y) = source_pos.bottom_center();
            let (tgt_x, tgt_y) = target_pos.top_center();

            let source_layer = (src_y / (target_pos.height + 40.0)).round() as usize;
            let target_layer = (tgt_y / (target_pos.height + 40.0)).round() as usize;

            edge_bundles
                .entry((source_layer, target_layer))
                .or_default()
                .push((dep.clone(), stage.id.clone(), src_x, tgt_x));
        }
    }

    for ((source_layer, target_layer), bundle) in edge_bundles {
        let bundle_size = bundle.len();
        let bundle_offset = if bundle_size > 1 {
            5.0
        } else {
            0.0
        };

        for (idx, (source_id, target_id, src_x, tgt_x)) in bundle.into_iter().enumerate() {
            let source_pos = positions.get(&source_id).unwrap();
            let target_pos = positions.get(&target_id).unwrap();

            let (_, src_y) = source_pos.bottom_center();
            let (_, tgt_y) = target_pos.top_center();

            let offset = if bundle_size > 1 {
                (idx as f64 - (bundle_size - 1) as f64 / 2.0) * bundle_offset
            } else {
                0.0
            };

            let mid_y = (src_y + tgt_y) / 2.0 + offset;

            let segments = if (src_x - tgt_x).abs() < 1.0 {
                vec![LineSegment::new(src_x, src_y, tgt_x, tgt_y)]
            } else {
                vec![
                    LineSegment::new(src_x, src_y, src_x, mid_y),
                    LineSegment::new(src_x, mid_y, tgt_x, mid_y),
                    LineSegment::new(tgt_x, mid_y, tgt_x, tgt_y),
                ]
            };

            edges.push(EdgePath::new(source_id, target_id, segments));

            let _ = (source_layer, target_layer);
        }
    }

    edges
}

/// Compute bounding box of the layout.
fn compute_bounds(positions: &HashMap<String, NodePosition>) -> LayoutBounds {
    if positions.is_empty() {
        return LayoutBounds::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for pos in positions.values() {
        min_x = min_x.min(pos.x);
        min_y = min_y.min(pos.y);
        max_x = max_x.max(pos.x + pos.width);
        max_y = max_y.max(pos.y + pos.height);
    }

    LayoutBounds::new(min_x, min_y, max_x, max_y)
}

/// Perform Sugiyama layout on a set of stages.
///
/// This implements the classic Sugiyama framework:
/// 1. Layer assignment based on dependencies (computed from Stage.dependencies)
/// 2. Node ordering via barycenter heuristic to minimize crossings
/// 3. Coordinate assignment with spacing constraints
/// 4. Orthogonal edge routing with bundling for parallel edges
///
/// # Arguments
/// * `stages` - Slice of stages to layout
///
/// # Returns
/// A `LayoutResult` containing node positions, edge paths, and bounds.
pub fn layout(stages: &[Stage]) -> LayoutResult {
    layout_with_config(stages, &LayoutConfig::default())
}

/// Perform Sugiyama layout with custom configuration.
pub fn layout_with_config(stages: &[Stage], config: &LayoutConfig) -> LayoutResult {
    if stages.is_empty() {
        return LayoutResult::new(HashMap::new(), Vec::new(), LayoutBounds::new(0.0, 0.0, 0.0, 0.0));
    }

    let layers = compute_layers(stages);
    let mut layer_groups = group_by_layer(stages, &layers);

    let _orders = order_by_barycenter(&mut layer_groups, stages, config.barycenter_iterations);

    let x_coords = assign_x_coordinates(&layer_groups, config);
    let positions = create_node_positions(stages, &layers, &x_coords, config);
    let edges = route_edges(stages, &positions);
    let bounds = compute_bounds(&positions);

    LayoutResult::new(positions, edges, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use chrono::Utc;

    fn make_stage(id: &str, deps: Vec<&str>) -> Stage {
        Stage {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            status: StageStatus::Queued,
            dependencies: deps.into_iter().map(String::from).collect(),
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            stage_type: Default::default(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
            auto_merge: None,
            working_dir: None,
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    fn test_empty_stages() {
        let result = layout(&[]);
        assert!(result.is_empty());
        assert_eq!(result.node_count(), 0);
        assert_eq!(result.edge_count(), 0);
    }

    #[test]
    fn test_single_stage() {
        let stages = vec![make_stage("a", vec![])];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 1);
        assert_eq!(result.edge_count(), 0);

        let pos = result.get_node("a").unwrap();
        assert!(pos.x >= 0.0);
        assert!(pos.y >= 0.0);
        assert!(pos.width > 0.0);
        assert!(pos.height > 0.0);
    }

    #[test]
    fn test_linear_chain_3_stages() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["a"]),
            make_stage("c", vec!["b"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 3);
        assert_eq!(result.edge_count(), 2);

        let pos_a = result.get_node("a").unwrap();
        let pos_b = result.get_node("b").unwrap();
        let pos_c = result.get_node("c").unwrap();

        assert!(pos_a.y < pos_b.y, "a should be above b");
        assert!(pos_b.y < pos_c.y, "b should be above c");

        assert!(
            (pos_a.center_x() - pos_b.center_x()).abs() < 1.0,
            "linear chain should be vertically aligned"
        );
        assert!(
            (pos_b.center_x() - pos_c.center_x()).abs() < 1.0,
            "linear chain should be vertically aligned"
        );
    }

    #[test]
    fn test_diamond_pattern_4_stages() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["a"]),
            make_stage("c", vec!["a"]),
            make_stage("d", vec!["b", "c"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 4);
        assert_eq!(result.edge_count(), 4);

        let pos_a = result.get_node("a").unwrap();
        let pos_b = result.get_node("b").unwrap();
        let pos_c = result.get_node("c").unwrap();
        let pos_d = result.get_node("d").unwrap();

        assert!(pos_a.y < pos_b.y, "a should be above b");
        assert!(pos_a.y < pos_c.y, "a should be above c");
        assert!(pos_b.y < pos_d.y, "b should be above d");
        assert!(pos_c.y < pos_d.y, "c should be above d");

        assert!(
            (pos_b.y - pos_c.y).abs() < 1.0,
            "b and c should be on same layer"
        );

        assert!(
            pos_b.x != pos_c.x,
            "b and c should have different x positions"
        );
    }

    #[test]
    fn test_wide_parallel_5_stages() {
        let stages = vec![
            make_stage("root", vec![]),
            make_stage("a", vec!["root"]),
            make_stage("b", vec!["root"]),
            make_stage("c", vec!["root"]),
            make_stage("d", vec!["root"]),
            make_stage("e", vec!["root"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 6);
        assert_eq!(result.edge_count(), 5);

        let pos_root = result.get_node("root").unwrap();
        let parallel_nodes = ["a", "b", "c", "d", "e"];

        for id in &parallel_nodes {
            let pos = result.get_node(id).unwrap();
            assert!(
                pos.y > pos_root.y,
                "{} should be below root",
                id
            );
        }

        let y_positions: Vec<f64> = parallel_nodes
            .iter()
            .map(|id| result.get_node(id).unwrap().y)
            .collect();

        let first_y = y_positions[0];
        for (idx, &y) in y_positions.iter().enumerate() {
            assert!(
                (y - first_y).abs() < 1.0,
                "parallel node {} should be on same layer",
                parallel_nodes[idx]
            );
        }

        let mut x_positions: Vec<f64> = parallel_nodes
            .iter()
            .map(|id| result.get_node(id).unwrap().x)
            .collect();
        x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for i in 1..x_positions.len() {
            assert!(
                x_positions[i] > x_positions[i - 1],
                "parallel nodes should have distinct x positions"
            );
        }
    }

    #[test]
    fn test_deep_sequential_10_layers() {
        let mut stages = vec![make_stage("s0", vec![])];
        for i in 1..10 {
            stages.push(make_stage(&format!("s{}", i), vec![&format!("s{}", i - 1)]));
        }

        let result = layout(&stages);

        assert_eq!(result.node_count(), 10);
        assert_eq!(result.edge_count(), 9);

        let mut prev_y = result.get_node("s0").unwrap().y;
        for i in 1..10 {
            let pos = result.get_node(&format!("s{}", i)).unwrap();
            assert!(
                pos.y > prev_y,
                "s{} should be below s{}",
                i,
                i - 1
            );
            prev_y = pos.y;
        }
    }

    #[test]
    fn test_complex_dag_mixed_patterns() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec![]),
            make_stage("c", vec!["a"]),
            make_stage("d", vec!["a", "b"]),
            make_stage("e", vec!["b"]),
            make_stage("f", vec!["c", "d"]),
            make_stage("g", vec!["d", "e"]),
            make_stage("h", vec!["f", "g"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 8);

        let pos_a = result.get_node("a").unwrap();
        let pos_b = result.get_node("b").unwrap();
        assert!(
            (pos_a.y - pos_b.y).abs() < 1.0,
            "a and b should be on same layer"
        );

        let pos_h = result.get_node("h").unwrap();
        for id in ["a", "b", "c", "d", "e", "f", "g"] {
            let pos = result.get_node(id).unwrap();
            assert!(pos.y < pos_h.y, "{} should be above h", id);
        }

        assert!(!result.edges().is_empty());
    }

    #[test]
    fn test_node_position_methods() {
        let pos = NodePosition::new(10.0, 20.0, 80.0, 30.0);

        assert_eq!(pos.center_x(), 50.0);
        assert_eq!(pos.center_y(), 35.0);
        assert_eq!(pos.bottom_center(), (50.0, 50.0));
        assert_eq!(pos.top_center(), (50.0, 20.0));
    }

    #[test]
    fn test_layout_bounds() {
        let bounds = LayoutBounds::new(10.0, 20.0, 100.0, 80.0);

        assert_eq!(bounds.width(), 90.0);
        assert_eq!(bounds.height(), 60.0);
    }

    #[test]
    fn test_layout_with_custom_config() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["a"]),
        ];

        let config = LayoutConfig {
            horizontal_spacing: 50.0,
            vertical_spacing: 100.0,
            node_width: 120.0,
            node_height: 50.0,
            barycenter_iterations: 2,
        };

        let result = layout_with_config(&stages, &config);

        let pos_a = result.get_node("a").unwrap();
        let pos_b = result.get_node("b").unwrap();

        assert_eq!(pos_a.width, 120.0);
        assert_eq!(pos_a.height, 50.0);

        // y for layer 0 = 0, y for layer 1 = node_height + vertical_spacing
        // gap = pos_b.y - (pos_a.y + pos_a.height) = (50 + 100) - (0 + 50) = 100
        let vertical_gap = pos_b.y - (pos_a.y + pos_a.height);
        assert!(
            (vertical_gap - config.vertical_spacing).abs() < 1.0,
            "vertical spacing should match config, got {} expected {}",
            vertical_gap,
            config.vertical_spacing
        );
    }

    #[test]
    fn test_edge_routing_straight() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["a"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.edge_count(), 1);

        let edge = &result.edges()[0];
        assert_eq!(edge.source, "a");
        assert_eq!(edge.target, "b");

        assert!(!edge.segments.is_empty());
    }

    #[test]
    fn test_edge_routing_orthogonal() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec![]),
            make_stage("c", vec!["a", "b"]),
        ];
        let result = layout(&stages);

        let pos_a = result.get_node("a").unwrap();
        let pos_c = result.get_node("c").unwrap();

        if (pos_a.center_x() - pos_c.center_x()).abs() > 1.0 {
            let edge_ac = result
                .edges()
                .iter()
                .find(|e| e.source == "a" && e.target == "c")
                .expect("edge a->c should exist");
            assert!(
                edge_ac.segments.len() >= 1,
                "non-aligned edge should have segments"
            );
        }
    }

    #[test]
    fn test_layout_result_iteration() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["a"]),
            make_stage("c", vec!["a"]),
        ];
        let result = layout(&stages);

        let node_ids: Vec<&String> = result.nodes().keys().collect();
        assert_eq!(node_ids.len(), 3);

        let edge_count = result.edges().len();
        assert_eq!(edge_count, 2);
    }

    #[test]
    fn test_cycle_handling() {
        let mut stage_a = make_stage("a", vec!["b"]);
        let mut stage_b = make_stage("b", vec!["a"]);

        stage_a.dependencies = vec!["b".to_string()];
        stage_b.dependencies = vec!["a".to_string()];

        let stages = vec![stage_a, stage_b];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 2);
    }

    #[test]
    fn test_missing_dependency() {
        let stages = vec![
            make_stage("a", vec![]),
            make_stage("b", vec!["nonexistent"]),
        ];
        let result = layout(&stages);

        assert_eq!(result.node_count(), 2);
    }

    #[test]
    fn test_barycenter_ordering_reduces_crossings() {
        let stages = vec![
            make_stage("a1", vec![]),
            make_stage("a2", vec![]),
            make_stage("b1", vec!["a1"]),
            make_stage("b2", vec!["a2"]),
        ];

        let config = LayoutConfig {
            barycenter_iterations: 4,
            ..Default::default()
        };

        let result = layout_with_config(&stages, &config);

        let pos_a1 = result.get_node("a1").unwrap();
        let pos_a2 = result.get_node("a2").unwrap();
        let pos_b1 = result.get_node("b1").unwrap();
        let pos_b2 = result.get_node("b2").unwrap();

        let a1_left = pos_a1.x < pos_a2.x;
        let b1_left = pos_b1.x < pos_b2.x;

        assert_eq!(
            a1_left, b1_left,
            "connected nodes should maintain relative ordering to minimize crossings"
        );
    }
}
