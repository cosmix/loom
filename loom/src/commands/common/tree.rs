//! Shared tree-rendering helpers used by multiple graph/status renderers (D-6).
//!
//! Canonical home for `compute_connector` and `format_dep_annotation`, imported
//! by `commands/graph/tree.rs` and `commands/status/render/graph.rs`.

use std::collections::HashMap;

use colored::{Color, Colorize};

/// Compute the tree connector prefix for a stage at a given level.
///
/// Indents 3 columns per level and uses `└─ ` for the last stage at any level,
/// `├─ ` otherwise. Root-level stages get only the indent with no connector.
pub fn compute_connector(level: usize, index_in_level: usize, level_size: usize) -> String {
    let indent = "   ".repeat(level);

    if level == 0 {
        indent
    } else if index_in_level == level_size - 1 {
        format!("{indent}└─ ")
    } else {
        format!("{indent}├─ ")
    }
}

/// Format an inline dependency annotation: `  ← dep1, dep2` placed right after
/// the stage id. Colors each dep using the shared color map. Returns an empty
/// string when the stage has no deps.
pub fn format_dep_annotation(deps: &[String], color_map: &HashMap<&str, Color>) -> String {
    if deps.is_empty() {
        return String::new();
    }

    let colored_deps: Vec<String> = deps
        .iter()
        .map(|dep| {
            if let Some(&color) = color_map.get(dep.as_str()) {
                format!("{}", dep.color(color))
            } else {
                dep.clone()
            }
        })
        .collect();

    format!("  {} {}", "←".dimmed(), colored_deps.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_connector_root() {
        assert_eq!(compute_connector(0, 0, 3), "");
    }

    #[test]
    fn test_compute_connector_non_last() {
        assert_eq!(compute_connector(1, 0, 3), "   ├─ ");
    }

    #[test]
    fn test_compute_connector_last() {
        assert_eq!(compute_connector(1, 2, 3), "   └─ ");
    }

    #[test]
    fn test_compute_connector_deep_indent() {
        let result = compute_connector(2, 0, 2);
        assert!(result.starts_with("      ")); // 2*3 spaces
        assert!(result.contains("├─"));
    }

    #[test]
    fn test_format_dep_annotation_empty() {
        let map: HashMap<&str, Color> = HashMap::new();
        assert_eq!(format_dep_annotation(&[], &map), "");
    }

    #[test]
    fn test_format_dep_annotation_known_dep() {
        let mut map: HashMap<&str, Color> = HashMap::new();
        map.insert("dep-a", Color::Green);
        let deps = vec!["dep-a".to_string()];
        let result = format_dep_annotation(&deps, &map);
        assert!(result.contains("dep-a"));
        assert!(result.contains("←"));
    }

    #[test]
    fn test_format_dep_annotation_unknown_dep() {
        let map: HashMap<&str, Color> = HashMap::new();
        let deps = vec!["unknown".to_string()];
        let result = format_dep_annotation(&deps, &map);
        assert!(result.contains("unknown"));
    }
}
