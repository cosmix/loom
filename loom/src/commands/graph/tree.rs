//! Vertical tree display for stage dependency graphs
//!
//! Renders stages as a vertical tree with connectors and dependency annotations.

use std::collections::HashMap;

use colored::Colorize;

use crate::models::stage::{Stage, StageStatus};

use super::colors::stage_color;
use super::indicators::status_indicator;
use super::levels::compute_stage_levels;

/// Compute the tree connector prefix based on position
fn compute_connector(index: usize, total: usize) -> &'static str {
    if index == 0 {
        ""
    } else if index == total - 1 {
        "└── "
    } else {
        "├── "
    }
}

/// Format dependency annotation right-aligned with colored dependency IDs
fn format_dep_annotation(
    deps: &[String],
    max_width: usize,
    current_width: usize,
    stage_map: &HashMap<&str, &Stage>,
) -> String {
    if deps.is_empty() {
        return String::new();
    }
    let padding = max_width.saturating_sub(current_width) + 10;

    // Color each dependency ID with its stage color
    let colored_deps: Vec<String> = deps
        .iter()
        .map(|dep| {
            if stage_map.contains_key(dep.as_str()) {
                format!("{}", dep.color(stage_color(dep)))
            } else {
                dep.clone()
            }
        })
        .collect();

    format!("{:width$}← {}", "", colored_deps.join(", "), width = padding)
}

/// Render footer showing currently running and next ready stages
fn render_footer(stages: &[Stage], stage_map: &HashMap<&str, &Stage>) -> String {
    let mut footer = String::new();

    // Find currently executing stage
    if let Some(executing) = stages.iter().find(|s| s.status == StageStatus::Executing) {
        let colored_name = executing.name.color(stage_color(&executing.id));
        let colored_id = executing.id.color(stage_color(&executing.id));
        footer.push_str(&format!(
            "{} Running:  {colored_name} ({colored_id})\n",
            "▶".cyan().bold()
        ));
    }

    // Find next queued stage
    if let Some(queued) = stages.iter().find(|s| s.status == StageStatus::Queued) {
        let colored_name = queued.name.color(stage_color(&queued.id));
        let colored_id = queued.id.color(stage_color(&queued.id));
        let incomplete_deps: Vec<String> = queued
            .dependencies
            .iter()
            .filter(|dep| {
                stage_map
                    .get(dep.as_str())
                    .is_none_or(|s| s.status != StageStatus::Completed)
            })
            .map(|dep| format!("{}", dep.color(stage_color(dep))))
            .collect();

        if incomplete_deps.is_empty() {
            footer.push_str(&format!(
                "{} Next:     {colored_name} ({colored_id})\n",
                "○".white().dimmed()
            ));
        } else {
            footer.push_str(&format!(
                "{} Next:     {colored_name} ({colored_id}) (blocked by: {})\n",
                "○".white().dimmed(),
                incomplete_deps.join(", ")
            ));
        }
    }

    footer
}

/// Build a vertical tree display of stages
pub fn build_tree_display(stages: &[Stage]) -> String {
    if stages.is_empty() {
        return "(no stages found)".to_string();
    }

    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let levels = compute_stage_levels(stages);

    // Sort stages by level ASC, then id ASC
    let mut sorted_stages: Vec<&Stage> = stages.iter().collect();
    sorted_stages.sort_by(|a, b| {
        let level_a = levels.get(&a.id).copied().unwrap_or(0);
        let level_b = levels.get(&b.id).copied().unwrap_or(0);
        level_a.cmp(&level_b).then_with(|| a.id.cmp(&b.id))
    });

    let max_name_width = sorted_stages.iter().map(|s| s.name.len()).max().unwrap_or(0);
    let total_stages = sorted_stages.len();

    let mut output = String::new();

    for (index, stage) in sorted_stages.iter().enumerate() {
        let connector = compute_connector(index, total_stages);
        let indicator = status_indicator(&stage.status);
        let display_width = stage.name.len() + stage.id.len() + 3; // " (id)"
        let deps = format_dep_annotation(&stage.dependencies, max_name_width + 20, display_width, &stage_map);
        let colored_name = stage.name.color(stage_color(&stage.id));
        let colored_id = stage.id.color(stage_color(&stage.id));
        output.push_str(&format!("{connector}{indicator} {colored_name} ({colored_id}){deps}\n"));
    }

    output.push_str(&"─".repeat(50));
    output.push('\n');

    output.push_str(&render_footer(stages, &stage_map));

    output
}
