//! Tree-based execution graph widget for TUI display
//!
//! Renders stages as a vertical tree with connectors and dependency annotations,
//! matching the display format used by `loom graph show`.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::theme::{StatusColors, Theme};
use crate::models::constants::display::{CONTEXT_HEALTHY_PCT, CONTEXT_WARNING_PCT};
use crate::models::stage::{Stage, StageStatus};
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
    use std::collections::HashSet;

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

/// Get status indicator character
fn status_char(status: &StageStatus) -> &'static str {
    match status {
        StageStatus::Completed => "✓",
        StageStatus::Executing => "●",
        StageStatus::Queued => "▶",
        StageStatus::WaitingForDeps => "○",
        StageStatus::WaitingForInput => "?",
        StageStatus::Blocked => "✗",
        StageStatus::NeedsHandoff => "⟳",
        StageStatus::Skipped => "⊘",
        StageStatus::MergeConflict => "⚡",
        StageStatus::CompletedWithFailures => "✗",
        StageStatus::MergeBlocked => "⚠",
    }
}

/// Get style for a stage status indicator
fn status_style(status: &StageStatus) -> Style {
    match status {
        StageStatus::Completed => Style::default()
            .fg(StatusColors::COMPLETED)
            .add_modifier(Modifier::BOLD),
        StageStatus::Executing => Style::default()
            .fg(StatusColors::EXECUTING)
            .add_modifier(Modifier::BOLD),
        StageStatus::Queued => Style::default()
            .fg(StatusColors::QUEUED)
            .add_modifier(Modifier::BOLD),
        StageStatus::WaitingForDeps => Theme::dimmed(),
        StageStatus::WaitingForInput => Style::default()
            .fg(StatusColors::WARNING)
            .add_modifier(Modifier::BOLD),
        StageStatus::Blocked => Style::default()
            .fg(StatusColors::BLOCKED)
            .add_modifier(Modifier::BOLD),
        StageStatus::NeedsHandoff => Style::default()
            .fg(StatusColors::WARNING)
            .add_modifier(Modifier::BOLD),
        StageStatus::Skipped => Theme::dimmed(),
        StageStatus::MergeConflict => Style::default()
            .fg(StatusColors::WARNING)
            .add_modifier(Modifier::BOLD),
        StageStatus::CompletedWithFailures => Style::default()
            .fg(StatusColors::BLOCKED)
            .add_modifier(Modifier::BOLD),
        StageStatus::MergeBlocked => Style::default()
            .fg(StatusColors::BLOCKED)
            .add_modifier(Modifier::BOLD),
    }
}

/// Tree-based execution graph widget
pub struct TreeWidget<'a> {
    stages: &'a [Stage],
    block: Option<Block<'a>>,
    context_percentages: HashMap<String, f32>,
    elapsed_times: HashMap<String, i64>,
}

impl<'a> TreeWidget<'a> {
    pub fn new(stages: &'a [Stage]) -> Self {
        Self {
            stages,
            block: None,
            context_percentages: HashMap::new(),
            elapsed_times: HashMap::new(),
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn context_percentages(mut self, percentages: HashMap<String, f32>) -> Self {
        self.context_percentages = percentages;
        self
    }

    pub fn elapsed_times(mut self, times: HashMap<String, i64>) -> Self {
        self.elapsed_times = times;
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

        let max_id_width = sorted_stages.iter().map(|s| s.id.len()).max().unwrap_or(0);

        let mut lines = Vec::new();

        for (global_index, stage) in sorted_stages.iter().enumerate() {
            let level = levels.get(&stage.id).copied().unwrap_or(0);
            let index_in_level = *level_indices.entry(level).or_insert(0);
            let level_size = level_counts.get(&level).copied().unwrap_or(1);
            let is_last_level = level == max_level;

            let mut spans = Vec::new();

            // Tree connector
            let indent = "    ".repeat(level);
            let connector = if level == 0 {
                indent.clone()
            } else if is_last_level && index_in_level == level_size - 1 {
                format!("{indent}└── ")
            } else {
                format!("{indent}├── ")
            };
            spans.push(Span::styled(connector, Theme::dimmed()));

            // Status indicator
            let indicator = status_char(&stage.status);
            spans.push(Span::styled(
                indicator.to_string(),
                status_style(&stage.status),
            ));
            spans.push(Span::raw(" "));

            // Stage ID with color
            let stage_color = color_by_index(global_index);
            spans.push(Span::styled(
                stage.id.clone(),
                Style::default().fg(stage_color),
            ));

            // Dependency annotation
            if !stage.dependencies.is_empty() {
                let padding = max_id_width.saturating_sub(stage.id.len()) + 2;
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::styled("← ", Theme::dimmed()));

                let dep_spans: Vec<Span> = stage
                    .dependencies
                    .iter()
                    .enumerate()
                    .flat_map(|(i, dep)| {
                        let mut dep_span_list = Vec::new();
                        if i > 0 {
                            dep_span_list.push(Span::styled(", ", Theme::dimmed()));
                        }
                        let dep_color =
                            color_map.get(dep.as_str()).copied().unwrap_or(Color::White);
                        dep_span_list
                            .push(Span::styled(dep.clone(), Style::default().fg(dep_color)));
                        dep_span_list
                    })
                    .collect();
                spans.extend(dep_spans);
            }

            // Context percentage and elapsed time for executing stages
            if matches!(stage.status, StageStatus::Executing) {
                if let Some(&ctx_pct) = self.context_percentages.get(&stage.id) {
                    let ctx_str = format!(" [{:.0}%]", ctx_pct * 100.0);
                    let ctx_style = if ctx_pct * 100.0 >= CONTEXT_WARNING_PCT {
                        Style::default().fg(StatusColors::CONTEXT_HIGH)
                    } else if ctx_pct * 100.0 >= CONTEXT_HEALTHY_PCT {
                        Style::default().fg(StatusColors::CONTEXT_MED)
                    } else {
                        Theme::dimmed()
                    };
                    spans.push(Span::styled(ctx_str, ctx_style));
                }

                if let Some(&secs) = self.elapsed_times.get(&stage.id) {
                    let elapsed = format_elapsed(secs);
                    spans.push(Span::styled(format!(" {elapsed}"), Theme::dimmed()));
                }
            }

            lines.push(Line::from(spans));

            // Increment index for this level
            *level_indices.get_mut(&level).unwrap() += 1;

            // Show base branch info for executing or queued stages
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
    fn render(self, area: Rect, buf: &mut Buffer) {
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
mod tests {
    use super::*;
    use chrono::Utc;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn make_stage(id: &str, deps: Vec<&str>, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            status,
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
            started_at: None,
            duration_secs: None,
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
            verification_status: Default::default(),
            context_budget: None,
        }
    }

    #[test]
    fn test_empty_stages() {
        let widget = TreeWidget::new(&[]);
        let lines = widget.build_lines();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_single_stage() {
        let stages = vec![make_stage("bootstrap", vec![], StageStatus::Completed)];
        let widget = TreeWidget::new(&stages);
        let lines = widget.build_lines();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_linear_dependency() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
            make_stage("c", vec!["b"], StageStatus::WaitingForDeps),
        ];
        let widget = TreeWidget::new(&stages);
        let lines = widget.build_lines();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_diamond_dependency() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Executing),
        ];
        let widget = TreeWidget::new(&stages);
        let lines = widget.build_lines();
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_widget_render() {
        let stages = vec![make_stage("test", vec![], StageStatus::Completed)];
        let widget = execution_tree(&stages);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // The border should be rendered
        assert_ne!(buf[(0, 0)].symbol(), " ");
    }

    #[test]
    fn test_stage_levels_computed_correctly() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Completed),
        ];
        let levels = compute_stage_levels(&stages);
        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&1));
        assert_eq!(levels.get("c"), Some(&1));
        assert_eq!(levels.get("d"), Some(&2));
    }

    #[test]
    fn test_status_indicators() {
        assert_eq!(status_char(&StageStatus::Completed), "✓");
        assert_eq!(status_char(&StageStatus::Executing), "●");
        assert_eq!(status_char(&StageStatus::Blocked), "✗");
    }

    #[test]
    fn test_with_context_and_elapsed() {
        let stages = vec![make_stage("exec", vec![], StageStatus::Executing)];

        let mut ctx = HashMap::new();
        ctx.insert("exec".to_string(), 0.45);

        let mut elapsed = HashMap::new();
        elapsed.insert("exec".to_string(), 120_i64);

        let widget = TreeWidget::new(&stages)
            .context_percentages(ctx)
            .elapsed_times(elapsed);

        let lines = widget.build_lines();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_with_base_branch() {
        let mut stage = make_stage("exec", vec!["root"], StageStatus::Executing);
        stage.base_branch = Some("loom/exec".to_string());
        let stages = vec![make_stage("root", vec![], StageStatus::Completed), stage];

        let widget = TreeWidget::new(&stages);
        let lines = widget.build_lines();
        // Should have 2 lines for root and exec, plus 1 for base branch info
        assert_eq!(lines.len(), 3);
    }
}
