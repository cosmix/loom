//! Tree-based execution graph widget for TUI display
//!
//! Renders stages as a vertical tree with connectors, dependency annotations,
//! and elapsed time for executing and completed stages.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::theme::{StatusColors, Theme};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::graph::levels;
use crate::utils::format_elapsed;

/// Available terminal colors for stage differentiation
const STAGE_COLORS: [Color; 16] = [
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::Rgb(255, 165, 0),   // Orange
    Color::Rgb(128, 0, 128),   // Purple
    Color::Rgb(0, 128, 128),   // Teal
    Color::Rgb(255, 192, 203), // Pink
];

/// Get a color by index, cycling through the palette
fn color_by_index(index: usize) -> Color {
    STAGE_COLORS[index % STAGE_COLORS.len()]
}

/// Compute topological level for each stage (level = max(dep_levels) + 1)
fn compute_stage_levels(stages: &[Stage]) -> HashMap<String, usize> {
    levels::compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies)
}

/// Compute elapsed time string for a stage based on its status and timestamps
fn compute_stage_elapsed(stage: &Stage) -> Option<String> {
    match stage.status {
        StageStatus::Executing => stage.started_at.map(|start| {
            let secs = chrono::Utc::now()
                .signed_duration_since(start)
                .num_seconds();
            format_elapsed(secs)
        }),
        StageStatus::Completed | StageStatus::CompletedWithFailures | StageStatus::Skipped => {
            if let Some(d) = stage.duration_secs {
                Some(format_elapsed(d))
            } else {
                match (stage.started_at, stage.completed_at) {
                    (Some(start), Some(end)) => Some(format_elapsed(
                        end.signed_duration_since(start).num_seconds(),
                    )),
                    _ => None,
                }
            }
        }
        _ => None,
    }
}

/// Truncate a string to fit within budget, appending ".." if truncated
fn truncate_id(id: &str, budget: usize) -> String {
    if id.len() <= budget {
        id.to_string()
    } else if budget >= 5 {
        format!("{}..", &id[..budget - 2])
    } else if budget > 0 {
        id[..budget].to_string()
    } else {
        String::new()
    }
}

/// Tree-based execution graph widget
pub struct TreeWidget<'a> {
    stages: &'a [Stage],
    block: Option<Block<'a>>,
    max_width: Option<usize>,
}

impl<'a> TreeWidget<'a> {
    pub fn new(stages: &'a [Stage]) -> Self {
        Self {
            stages,
            block: None,
            max_width: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn max_width(mut self, width: usize) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Build tree lines for rendering
    pub fn build_lines(&self) -> Vec<Line<'a>> {
        if self.stages.is_empty() {
            return vec![Line::from(Span::styled("(no stages)", Theme::dimmed()))];
        }

        let levels = compute_stage_levels(self.stages);

        // Sort stages by level ASC, then id ASC
        let mut sorted_stages: Vec<&Stage> = self.stages.iter().collect();
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

        let max_w = self.max_width.unwrap_or(200);
        let mut lines = Vec::new();

        for (global_index, stage) in sorted_stages.iter().enumerate() {
            let level = levels.get(&stage.id).copied().unwrap_or(0);
            let index_in_level = *level_indices.entry(level).or_insert(0);
            let level_size = level_counts.get(&level).copied().unwrap_or(1);
            let is_last_level = level == max_level;

            // Tree connector
            let indent = "    ".repeat(level);
            let connector = if level == 0 {
                indent.clone()
            } else if is_last_level && index_in_level == level_size - 1 {
                format!("{indent}└── ")
            } else {
                format!("{indent}├── ")
            };

            // Pre-compute parts for width calculation
            let elapsed = compute_stage_elapsed(stage);
            let elapsed_text = elapsed
                .as_ref()
                .map(|s| format!("  {s}"))
                .unwrap_or_default();

            let deps_joined = stage.dependencies.join(", ");
            let deps_prefix = if stage.dependencies.is_empty() {
                ""
            } else {
                "  ← "
            };

            // Width accounting
            let connector_w = connector.chars().count();
            let icon_w = 2; // icon char + space
            let elapsed_w = elapsed_text.chars().count();
            let deps_w = deps_prefix.len() + deps_joined.len();

            // Budget for stage ID (runtime always visible, deps can shrink)
            let fixed_w = connector_w + icon_w + elapsed_w + deps_w;
            let id_budget = max_w.saturating_sub(fixed_w).max(8);
            let id_display = truncate_id(&stage.id, id_budget);

            // Recalculate available space for deps after ID truncation
            let used_w = connector_w + icon_w + id_display.len() + elapsed_w;
            let deps_budget = max_w.saturating_sub(used_w);

            // Build spans
            let stage_color = color_by_index(global_index);
            let mut spans = vec![
                Span::styled(connector, Theme::dimmed()),
                Span::styled(stage.status.icon().to_string(), stage.status.tui_style()),
                Span::raw(" "),
                Span::styled(id_display, Style::default().fg(stage_color)),
            ];

            // Elapsed time (for executing and completed stages)
            if let Some(ref el) = elapsed {
                spans.push(Span::styled(format!("  {el}"), Theme::dimmed()));
            }

            // Dependencies
            if !stage.dependencies.is_empty() && deps_budget >= 7 {
                spans.push(Span::styled("  ← ", Theme::dimmed()));
                let deps_content_budget = deps_budget.saturating_sub(4);

                if deps_joined.len() <= deps_content_budget {
                    // Full deps with colors
                    for (i, dep) in stage.dependencies.iter().enumerate() {
                        if i > 0 {
                            spans.push(Span::styled(", ", Theme::dimmed()));
                        }
                        let dep_color =
                            color_map.get(dep.as_str()).copied().unwrap_or(Color::White);
                        spans.push(Span::styled(dep.clone(), Style::default().fg(dep_color)));
                    }
                } else if deps_content_budget >= 5 {
                    let truncated = format!(
                        "{}..",
                        &deps_joined[..deps_content_budget.saturating_sub(2)]
                    );
                    spans.push(Span::styled(truncated, Theme::dimmed()));
                } else {
                    spans.push(Span::styled("...", Theme::dimmed()));
                }
            }

            lines.push(Line::from(spans));

            // Increment index for this level
            *level_indices.get_mut(&level).unwrap() += 1;

            // Base branch info for executing or queued stages (if available)
            if matches!(stage.status, StageStatus::Executing | StageStatus::Queued) {
                if let Some(ref base_branch) = stage.base_branch {
                    let mut base_spans = Vec::new();
                    let base_indent = "    ".repeat(level + 1);
                    base_spans.push(Span::styled(base_indent, Style::default()));
                    base_spans.push(Span::styled("Base: ", Theme::dimmed()));
                    base_spans.push(Span::styled(
                        base_branch.clone(),
                        Style::default().fg(StatusColors::QUEUED),
                    ));

                    if !stage.base_merged_from.is_empty() {
                        let merged_deps: Vec<Span> = stage
                            .base_merged_from
                            .iter()
                            .enumerate()
                            .flat_map(|(i, dep)| {
                                let mut list = Vec::new();
                                if i > 0 {
                                    list.push(Span::styled(", ", Theme::dimmed()));
                                }
                                let dep_color =
                                    color_map.get(dep.as_str()).copied().unwrap_or(Color::White);
                                list.push(Span::styled(
                                    dep.clone(),
                                    Style::default().fg(dep_color),
                                ));
                                list
                            })
                            .collect();
                        base_spans.push(Span::styled(" (merged from: ", Theme::dimmed()));
                        base_spans.extend(merged_deps);
                        base_spans.push(Span::styled(")", Theme::dimmed()));
                    } else if let Some(dep_id) = stage.dependencies.first() {
                        let dep_color = color_map
                            .get(dep_id.as_str())
                            .copied()
                            .unwrap_or(Color::White);
                        base_spans.push(Span::styled(" (inherited from ", Theme::dimmed()));
                        base_spans
                            .push(Span::styled(dep_id.clone(), Style::default().fg(dep_color)));
                        base_spans.push(Span::styled(")", Theme::dimmed()));
                    }

                    lines.push(Line::from(base_spans));
                }
            }
        }

        lines
    }
}

impl Widget for TreeWidget<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // Apply block if present
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner_area.width < 2 || inner_area.height < 1 {
            return;
        }

        // Auto-set max_width from render area if not explicitly set
        if self.max_width.is_none() {
            self.max_width = Some(inner_area.width as usize);
        }

        let lines = self.build_lines();
        let paragraph = Paragraph::new(lines);
        paragraph.render(inner_area, buf);
    }
}

/// Create a tree widget with default styling
pub fn execution_tree(stages: &[Stage]) -> TreeWidget<'_> {
    TreeWidget::new(stages).block(
        Block::default()
            .title(" Execution Graph ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(StatusColors::BORDER)),
    )
}

#[cfg(test)]
#[path = "tree_widget_tests.rs"]
mod tests;
