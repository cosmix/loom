//! Level computation for stage dependency graphs.
//!
//! Computes topological levels for stages where a stage's level is
//! max(dependency_levels) + 1. Root stages (no dependencies) are at level 0.

use std::collections::{HashMap, HashSet};

/// Compute topological level for a single stage recursively.
///
/// This is a generic implementation that works with any type `T` that has:
/// - An ID (string slice)
/// - A list of dependencies (vector of strings)
///
/// The level is computed as:
/// - Level 0 if the stage has no dependencies
/// - max(dependency_levels) + 1 otherwise
///
/// Includes cycle detection to avoid infinite recursion.
fn compute_level_impl<'a, T, F>(
    stage_id: &str,
    stage_map: &HashMap<&str, &'a T>,
    levels: &mut HashMap<String, usize>,
    visiting: &mut HashSet<String>,
    get_dependencies: &F,
) -> usize
where
    F: Fn(&'a T) -> &'a [String],
{
    // Return cached level if already computed
    if let Some(&level) = levels.get(stage_id) {
        return level;
    }

    // Cycle detection - treat as level 0 to avoid infinite recursion
    if visiting.contains(stage_id) {
        return 0;
    }
    visiting.insert(stage_id.to_string());

    // Look up the stage
    let stage = match stage_map.get(stage_id) {
        Some(s) => s,
        None => return 0,
    };

    // Compute level based on dependencies
    let dependencies = get_dependencies(stage);
    let level = if dependencies.is_empty() {
        0
    } else {
        dependencies
            .iter()
            .map(|dep| compute_level_impl(dep, stage_map, levels, visiting, get_dependencies))
            .max()
            .unwrap_or(0)
            + 1
    };

    // Remove from visiting set and cache the result
    visiting.remove(stage_id);
    levels.insert(stage_id.to_string(), level);
    level
}

/// Compute levels for all stages in a collection.
///
/// Returns a HashMap mapping stage IDs to their computed levels.
pub fn compute_all_levels<'a, T, I, D>(
    stages: &'a [T],
    get_id: I,
    get_dependencies: D,
) -> HashMap<String, usize>
where
    I: Fn(&'a T) -> &'a str,
    D: Fn(&'a T) -> &'a [String],
{
    let stage_map: HashMap<&str, &T> = stages.iter().map(|s| (get_id(s), s)).collect();
    let mut levels: HashMap<String, usize> = HashMap::new();

    for stage in stages {
        let mut visiting = HashSet::new();
        compute_level_impl(
            get_id(stage),
            &stage_map,
            &mut levels,
            &mut visiting,
            &get_dependencies,
        );
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestStage {
        id: String,
        dependencies: Vec<String>,
    }

    #[test]
    fn test_compute_level_no_dependencies() {
        let stages = vec![TestStage {
            id: "root".to_string(),
            dependencies: vec![],
        }];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        assert_eq!(levels.get("root"), Some(&0));
    }

    #[test]
    fn test_compute_level_single_dependency() {
        let stages = vec![
            TestStage {
                id: "root".to_string(),
                dependencies: vec![],
            },
            TestStage {
                id: "child".to_string(),
                dependencies: vec!["root".to_string()],
            },
        ];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        assert_eq!(levels.get("root"), Some(&0));
        assert_eq!(levels.get("child"), Some(&1));
    }

    #[test]
    fn test_compute_level_multiple_dependencies() {
        let stages = vec![
            TestStage {
                id: "a".to_string(),
                dependencies: vec![],
            },
            TestStage {
                id: "b".to_string(),
                dependencies: vec![],
            },
            TestStage {
                id: "c".to_string(),
                dependencies: vec!["a".to_string(), "b".to_string()],
            },
        ];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&0));
        assert_eq!(levels.get("c"), Some(&1));
    }

    #[test]
    fn test_compute_level_deep_chain() {
        let stages = vec![
            TestStage {
                id: "a".to_string(),
                dependencies: vec![],
            },
            TestStage {
                id: "b".to_string(),
                dependencies: vec!["a".to_string()],
            },
            TestStage {
                id: "c".to_string(),
                dependencies: vec!["b".to_string()],
            },
        ];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&1));
        assert_eq!(levels.get("c"), Some(&2));
    }

    #[test]
    fn test_compute_level_cycle_detection() {
        // Create a cycle: a -> b -> c -> a
        let stages = vec![
            TestStage {
                id: "a".to_string(),
                dependencies: vec!["c".to_string()],
            },
            TestStage {
                id: "b".to_string(),
                dependencies: vec!["a".to_string()],
            },
            TestStage {
                id: "c".to_string(),
                dependencies: vec!["b".to_string()],
            },
        ];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        // With cycle detection, all should get some level
        assert!(levels.contains_key("a"));
        assert!(levels.contains_key("b"));
        assert!(levels.contains_key("c"));
    }

    #[test]
    fn test_compute_level_missing_dependency() {
        let stages = vec![TestStage {
            id: "orphan".to_string(),
            dependencies: vec!["nonexistent".to_string()],
        }];

        let levels = compute_all_levels(&stages, |s| &s.id, |s| &s.dependencies);

        // Should handle missing dependency gracefully
        assert_eq!(levels.get("orphan"), Some(&1));
    }
}
