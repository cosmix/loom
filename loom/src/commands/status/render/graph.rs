//! Tree-based execution graph display for status command
//!
//! Renders stages as a vertical tree with connectors and dependency annotations,
//! matching the display format used by `loom graph show`.

use std::collections::HashMap;
use std::io::Write;

use colored::{Color, Colorize};

use crate::commands::status::data::{StageSummary, StatusData};
use crate::models::constants::display::{CONTEXT_HEALTHY_PCT, CONTEXT_WARNING_PCT};
use crate::models::stage::StageStatus;
use crate::utils::format_elapsed;

/// Available terminal colors for stage differentiation
const STAGE_COLORS: [Color; 16] = [
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::BrightRed,
    Color::BrightGreen,
    Color::BrightYellow,
    Color::BrightBlue,
    Color::BrightMagenta,
    Color::BrightCyan,
    Color::TrueColor {
        r: 255,
        g: 165,
        b: 0,
    }, // Orange
    Color::TrueColor {
        r: 128,
        g: 0,
        b: 128,
    }, // Purple
    Color::TrueColor {
        r: 0,
        g: 128,
        b: 128,
    }, // Teal
    Color::TrueColor {
        r: 255,
        g: 192,
        b: 203,
    }, // Pink
];

/// Get a color by index, cycling through the palette
fn color_by_index(index: usize) -> Color {
    STAGE_COLORS[index % STAGE_COLORS.len()]
}

/// Compute topological level for each stage (level = max(dep_levels) + 1)
fn compute_stage_levels(stages: &[StageSummary]) -> HashMap<String, usize> {
    use std::collections::HashSet;

    let stage_map: HashMap<&str, &StageSummary> =
        stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut levels: HashMap<String, usize> = HashMap::new();

    fn get_level(
        stage_id: &str,
        stage_map: &HashMap<&str, &StageSummary>,
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

/// Status indicator with color for display
fn status_indicator(status: &StageStatus) -> colored::ColoredString {
    match status {
        StageStatus::Completed => "✓".green().bold(),
        StageStatus::Executing => "●".blue().bold(),
        StageStatus::Queued => "▶".cyan().bold(),
        StageStatus::WaitingForDeps => "○".white().dimmed(),
        StageStatus::WaitingForInput => "?".magenta().bold(),
        StageStatus::Blocked => "✗".red().bold(),
        StageStatus::NeedsHandoff => "⟳".yellow().bold(),
        StageStatus::Skipped => "⊘".white().dimmed(),
        StageStatus::MergeConflict => "⚡".yellow().bold(),
        StageStatus::CompletedWithFailures => "✗".red().bold(),
        StageStatus::MergeBlocked => "⚠".red().bold(),
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

    format!(
        "{:width$}← {}",
        "",
        colored_deps.join(", "),
        width = padding
    )
}

/// Format base branch info for a stage
fn format_base_branch_info(
    stage: &StageSummary,
    color_map: &HashMap<&str, Color>,
) -> Option<String> {
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

/// Render execution graph with tree display
pub fn render_graph<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    writeln!(w, "{}", "Execution Graph".bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    if data.stages.is_empty() {
        writeln!(w, "  {}", "(no stages found)".dimmed())?;
        writeln!(w)?;
        render_legend(w)?;
        return Ok(());
    }

    let levels = compute_stage_levels(&data.stages);

    // Sort stages by level ASC, then id ASC
    let mut sorted_stages: Vec<&StageSummary> = data.stages.iter().collect();
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

    for (global_index, stage) in sorted_stages.iter().enumerate() {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        let index_in_level = *level_indices.entry(level).or_insert(0);
        let level_size = level_counts.get(&level).copied().unwrap_or(1);
        let is_last_level = level == max_level;

        let connector = compute_connector(level, index_in_level, level_size, is_last_level);
        let indicator = status_indicator(&stage.status);
        let deps = format_dep_annotation(
            &stage.dependencies,
            max_id_width,
            stage.id.len(),
            &color_map,
        );
        let color = color_by_index(global_index);
        let colored_id = stage.id.color(color);

        // Build the main line: connector + indicator + stage_id + deps
        write!(w, "{connector}{indicator} {colored_id}{deps}")?;

        // Add context percentage and elapsed time for executing stages
        if matches!(stage.status, StageStatus::Executing) {
            if let Some(ctx_pct) = stage.context_pct {
                let ctx_str = format!(" [{:.0}%]", ctx_pct * 100.0);
                let colored_ctx = if ctx_pct * 100.0 >= CONTEXT_WARNING_PCT {
                    ctx_str.red()
                } else if ctx_pct * 100.0 >= CONTEXT_HEALTHY_PCT {
                    ctx_str.yellow()
                } else {
                    ctx_str.dimmed()
                };
                write!(w, "{colored_ctx}")?;
            }
            if let Some(secs) = stage.elapsed_secs {
                let elapsed = format_elapsed(secs);
                write!(w, " {}", elapsed.dimmed())?;
            }

            // Show activity status for executing stages
            let activity_icon = stage.activity_status.icon();
            write!(w, " {activity_icon}")?;

            // Add staleness warning
            if let Some(staleness) = stage.staleness_secs {
                if staleness > 300 {
                    write!(w, " {}", "(stale)".yellow())?;
                }
            }
        }

        writeln!(w)?;

        // Increment index for this level
        *level_indices.get_mut(&level).unwrap() += 1;

        // Show base branch info for executing or queued stages with base branch set
        if matches!(stage.status, StageStatus::Executing | StageStatus::Queued) {
            if let Some(base_info) = format_base_branch_info(stage, &color_map) {
                writeln!(w, "{base_info}")?;
            }
        }
    }

    writeln!(w)?;
    render_legend(w)?;

    Ok(())
}

/// Render the legend explaining status indicators
fn render_legend<W: Write>(w: &mut W) -> std::io::Result<()> {
    write!(w, "  {} ", "Legend:".dimmed())?;
    write!(w, "{} ", "✓".green())?;
    write!(w, "done  ")?;
    write!(w, "{} ", "●".blue())?;
    write!(w, "exec  ")?;
    write!(w, "{} ", "▶".cyan())?;
    write!(w, "ready  ")?;
    write!(w, "{} ", "○".dimmed())?;
    write!(w, "wait  ")?;
    write!(w, "{} ", "✗".red())?;
    write!(w, "blocked  ")?;
    write!(w, "{} ", "⟳".yellow())?;
    writeln!(w, "handoff")?;
    Ok(())
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
