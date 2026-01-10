//! Topological level computation for stage graphs
//!
//! Computes the dependency depth (level) for each stage in the execution graph.

use std::collections::{HashMap, HashSet};

use crate::models::stage::Stage;

/// Compute the topological level for each stage.
/// Level = max(levels of all dependencies) + 1, with roots at level 0.
pub fn compute_stage_levels(stages: &[Stage]) -> HashMap<String, usize> {
    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut levels: HashMap<String, usize> = HashMap::new();

    fn get_level(
        stage_id: &str,
        stage_map: &HashMap<&str, &Stage>,
        levels: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> usize {
        if let Some(&level) = levels.get(stage_id) {
            return level;
        }

        // Cycle detection - treat as level 0 to avoid infinite recursion
        if visiting.contains(stage_id) {
            return 0;
        }
        visiting.insert(stage_id.to_string());

        let stage = match stage_map.get(stage_id) {
            Some(s) => s,
            None => return 0,
        };

        let level = if stage.dependencies.is_empty() {
            0
        } else {
            stage
                .dependencies
                .iter()
                .map(|dep| get_level(dep, stage_map, levels, visiting))
                .max()
                .unwrap_or(0)
                + 1
        };

        visiting.remove(stage_id);
        levels.insert(stage_id.to_string(), level);
        level
    }

    for stage in stages {
        let mut visiting = HashSet::new();
        get_level(&stage.id, &stage_map, &mut levels, &mut visiting);
    }

    levels
}
