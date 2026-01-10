//! Graph display formatting
//!
//! Builds visual representations of the stage dependency graph.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;

use colored::Colorize;

use crate::models::stage::Stage;

use super::colors::stage_color;
use super::indicators::{status_indicator, status_priority};
use super::levels::compute_stage_levels;

/// Format dependencies with their status indicators
fn format_dependencies(stage: &Stage, stage_map: &HashMap<&str, &Stage>) -> String {
    if stage.dependencies.is_empty() {
        return String::new();
    }

    let dep_strs: Vec<String> = stage
        .dependencies
        .iter()
        .map(|dep_id| {
            if let Some(dep_stage) = stage_map.get(dep_id.as_str()) {
                let ind = status_indicator(&dep_stage.status);
                format!("{ind}{dep_id}")
            } else {
                format!("?{dep_id}")
            }
        })
        .collect();

    format!(" ← {}", dep_strs.join(", "))
}

/// Build a visual representation of the dependency graph using layered levels
pub fn build_graph_display(stages: &[Stage]) -> Result<String> {
    if stages.is_empty() {
        return Ok("(no stages found - run 'loom init <plan>' to create stages)".to_string());
    }

    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let levels = compute_stage_levels(stages);

    // Group stages by level (BTreeMap for sorted keys)
    let mut by_level: BTreeMap<usize, Vec<&Stage>> = BTreeMap::new();
    for stage in stages {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        by_level.entry(level).or_default().push(stage);
    }

    // Sort stages within each level by status priority, then by id
    for stages_in_level in by_level.values_mut() {
        stages_in_level.sort_by(|a, b| {
            status_priority(&a.status)
                .cmp(&status_priority(&b.status))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    let mut output = String::new();

    for (level, stages_in_level) in &by_level {
        // Level header
        let header = if *level == 0 {
            "Level 0 (no dependencies):".to_string()
        } else {
            format!("Level {level}:")
        };
        output.push_str(&header);
        output.push('\n');

        // Render each stage in this level
        for stage in stages_in_level {
            let indicator = status_indicator(&stage.status);
            let deps = format_dependencies(stage, &stage_map);
            let colored_name = stage.name.color(stage_color(&stage.id));
            output.push_str(&format!(
                "  {indicator} {colored_name} ({}){deps}\n",
                stage.id
            ));
        }

        output.push('\n');
    }

    Ok(output)
}
