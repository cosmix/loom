//! Vertical tree display for stage dependency graphs
//!
//! Renders stages as a vertical tree with connectors and dependency annotations.

use std::collections::HashMap;

use colored::{Color, Colorize};

use crate::models::stage::{Stage, StageStatus};

use super::colors::color_by_index;
use super::indicators::status_indicator;
use super::levels::compute_stage_levels;

/// Compute the tree connector prefix based on level and position within level
fn compute_connector(
    level: usize,
    index_in_level: usize,
    level_size: usize,
    is_last_level: bool,
) -> String {
    // Base indentation: 4 spaces per level
    let indent = "    ".repeat(level);

    if level == 0 {
        // Root level: no connectors, just indentation
        indent
    } else if is_last_level && index_in_level == level_size - 1 {
        // Last stage in the last level: use └──
        format!("{indent}└── ")
    } else {
        // Other stages at non-root levels: use ├──
        format!("{indent}├── ")
    }
}

/// Format dependency annotation right-aligned with colored dependency IDs
fn format_dep_annotation(
    deps: &[String],
    max_width: usize,
    current_width: usize,
    color_map: &HashMap<&str, Color>,
) -> String {
    if deps.is_empty() {
        return String::new();
    }
    let padding = max_width.saturating_sub(current_width) + 2;

    // Color each dependency ID with its assigned color from the map
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

    format!("{:width$}← {}", "", colored_deps.join(", "), width = padding)
}

/// Format base branch info for a stage
fn format_base_branch_info(stage: &Stage, color_map: &HashMap<&str, Color>) -> Option<String> {
    let base_branch = stage.base_branch.as_ref()?;

    let base_info = if stage.base_merged_from.is_empty() {
        // Single dependency - show which stage it inherited from
        if let Some(dep_id) = stage.dependencies.first() {
            let colored_dep = if let Some(&color) = color_map.get(dep_id.as_str()) {
                format!("{}", dep_id.color(color))
            } else {
                dep_id.clone()
            };
            format!(
                "  {} {} {}",
                "Base:".dimmed(),
                base_branch.cyan(),
                format!("(inherited from {colored_dep})").dimmed()
            )
        } else {
            format!("  {} {}", "Base:".dimmed(), base_branch.cyan())
        }
    } else {
        // Multiple dependencies - show merged sources
        let colored_sources: Vec<String> = stage
            .base_merged_from
            .iter()
            .map(|dep| {
                if let Some(&color) = color_map.get(dep.as_str()) {
                    format!("{}", dep.color(color))
                } else {
                    dep.clone()
                }
            })
            .collect();
        format!(
            "  {} {} {}",
            "Base:".dimmed(),
            base_branch.cyan(),
            format!("(merged from: {})", colored_sources.join(", ")).dimmed()
        )
    };

    Some(base_info)
}

/// Build a vertical tree display of stages
pub fn build_tree_display(stages: &[Stage]) -> String {
    if stages.is_empty() {
        return "(no stages found)".to_string();
    }

    let levels = compute_stage_levels(stages);

    // Sort stages by level ASC, then id ASC
    let mut sorted_stages: Vec<&Stage> = stages.iter().collect();
    sorted_stages.sort_by(|a, b| {
        let level_a = levels.get(&a.id).copied().unwrap_or(0);
        let level_b = levels.get(&b.id).copied().unwrap_or(0);
        level_a.cmp(&level_b).then_with(|| a.id.cmp(&b.id))
    });

    // Create position-based color map so adjacent stages have different colors
    let color_map: HashMap<&str, Color> = sorted_stages
        .iter()
        .enumerate()
        .map(|(i, stage)| (stage.id.as_str(), color_by_index(i)))
        .collect();

    // Calculate max level and count stages per level
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut level_counts: HashMap<usize, usize> = HashMap::new();
    let mut level_indices: HashMap<usize, usize> = HashMap::new();
    for stage in &sorted_stages {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        *level_counts.entry(level).or_insert(0) += 1;
    }

    let max_id_width = sorted_stages.iter().map(|s| s.id.len()).max().unwrap_or(0);

    let mut output = String::new();

    for (global_index, stage) in sorted_stages.iter().enumerate() {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        let index_in_level = *level_indices.entry(level).or_insert(0);
        let level_size = level_counts.get(&level).copied().unwrap_or(1);
        let is_last_level = level == max_level;

        let connector = compute_connector(level, index_in_level, level_size, is_last_level);
        let indicator = status_indicator(&stage.status);
        let deps = format_dep_annotation(&stage.dependencies, max_id_width, stage.id.len(), &color_map);
        let color = color_by_index(global_index);
        let colored_id = stage.id.color(color);
        output.push_str(&format!("{connector}{indicator} {colored_id}{deps}\n"));

        // Increment index for this level
        *level_indices.get_mut(&level).unwrap() += 1;

        // Show base branch info for executing or queued stages with base branch set
        if matches!(stage.status, StageStatus::Executing | StageStatus::Queued) {
            if let Some(base_info) = format_base_branch_info(stage, &color_map) {
                output.push_str(&format!("{base_info}\n"));
            }
        }
    }

    output
}
