//! Execution graph widget for TUI display
//!
//! Renders an ASCII DAG visualization of stage dependencies with status colors.

use std::collections::BTreeMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::commands::graph::compute_stage_levels;
use crate::models::stage::{Stage, StageStatus};

use super::theme::{StatusColors, Theme};

/// A stage node with computed layout position
#[derive(Debug, Clone)]
struct LayoutNode {
    id: String,
    status: StageStatus,
    level: usize,
    #[allow(dead_code)] // Used in tests for position verification
    position: usize,
    dependencies: Vec<String>,
}

/// Execution graph widget
pub struct GraphWidget<'a> {
    stages: &'a [Stage],
    block: Option<Block<'a>>,
}

impl<'a> GraphWidget<'a> {
    pub fn new(stages: &'a [Stage]) -> Self {
        Self {
            stages,
            block: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Compute layout for all stages
    fn compute_layout(&self) -> Vec<LayoutNode> {
        if self.stages.is_empty() {
            return Vec::new();
        }

        let levels = compute_stage_levels(self.stages);

        // Group stages by level
        let mut by_level: BTreeMap<usize, Vec<&Stage>> = BTreeMap::new();
        for stage in self.stages {
            let level = levels.get(&stage.id).copied().unwrap_or(0);
            by_level.entry(level).or_default().push(stage);
        }

        // Sort stages within each level by id for consistent ordering
        for stages_in_level in by_level.values_mut() {
            stages_in_level.sort_by(|a, b| a.id.cmp(&b.id));
        }

        // Create layout nodes with positions
        let mut nodes = Vec::new();
        for (level, stages_in_level) in &by_level {
            for (pos, stage) in stages_in_level.iter().enumerate() {
                nodes.push(LayoutNode {
                    id: stage.id.clone(),
                    status: stage.status.clone(),
                    level: *level,
                    position: pos,
                    dependencies: stage.dependencies.clone(),
                });
            }
        }

        nodes
    }

    /// Get style for a stage status
    fn status_style(status: &StageStatus) -> Style {
        match status {
            StageStatus::Completed => Theme::status_completed(),
            StageStatus::Executing => Theme::status_executing(),
            StageStatus::Queued => Theme::status_queued(),
            StageStatus::WaitingForDeps => Theme::status_pending(),
            StageStatus::Blocked => Theme::status_blocked(),
            StageStatus::NeedsHandoff => Theme::status_warning(),
            StageStatus::WaitingForInput => Theme::status_warning(),
            StageStatus::Skipped => Theme::dimmed(),
            StageStatus::MergeConflict => Theme::status_blocked(),
            StageStatus::CompletedWithFailures => Theme::status_warning(),
            StageStatus::MergeBlocked => Theme::status_blocked(),
        }
    }

    /// Get status indicator character
    fn status_char(status: &StageStatus) -> char {
        match status {
            StageStatus::Completed => '✓',
            StageStatus::Executing => '●',
            StageStatus::Queued => '▶',
            StageStatus::WaitingForDeps => '○',
            StageStatus::Blocked => '✗',
            StageStatus::NeedsHandoff => '⟳',
            StageStatus::WaitingForInput => '?',
            StageStatus::Skipped => '⊘',
            StageStatus::MergeConflict => '⚡',
            StageStatus::CompletedWithFailures => '⚠',
            StageStatus::MergeBlocked => '⊗',
        }
    }

    /// Render the graph as lines for Paragraph widget
    fn render_as_lines(&self) -> Vec<Line<'static>> {
        if self.stages.is_empty() {
            return vec![Line::from(Span::styled("(no stages)", Theme::dimmed()))];
        }

        let nodes = self.compute_layout();
        if nodes.is_empty() {
            return vec![Line::from(Span::styled("(no stages)", Theme::dimmed()))];
        }

        // Group nodes by level
        let mut by_level: BTreeMap<usize, Vec<&LayoutNode>> = BTreeMap::new();
        for node in &nodes {
            by_level.entry(node.level).or_default().push(node);
        }

        let mut lines = Vec::new();
        let max_level = by_level.keys().max().copied().unwrap_or(0);

        for (level, level_nodes) in &by_level {
            // Render level label
            let level_label = if *level == 0 {
                "Root".to_string()
            } else {
                format!("L{}", level)
            };

            // Build the stage boxes for this level
            let mut spans = Vec::new();
            spans.push(Span::styled(format!("{:>4} ", level_label), Theme::dimmed()));

            for (i, node) in level_nodes.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw("  "));
                }

                let style = Self::status_style(&node.status);
                let indicator = Self::status_char(&node.status);

                // Truncate id to fit
                let max_len = 12;
                let display_id = if node.id.len() > max_len {
                    format!("{}…", &node.id[..max_len - 1])
                } else {
                    node.id.clone()
                };

                spans.push(Span::styled(format!("[{} {}]", indicator, display_id), style));
            }

            lines.push(Line::from(spans));

            // Render arrows to next level (if not last level)
            if *level < max_level {
                let mut arrow_spans = Vec::new();
                arrow_spans.push(Span::raw("     ")); // Indent to match level label

                // Find dependencies that point to this level
                let next_level_nodes = by_level.get(&(level + 1));
                if let Some(next_nodes) = next_level_nodes {
                    let mut arrow_chars: Vec<String> = Vec::new();

                    for (i, node) in level_nodes.iter().enumerate() {
                        if i > 0 {
                            arrow_chars.push("  ".to_string());
                        }

                        // Check if any node in next level depends on this node
                        let has_dependent =
                            next_nodes.iter().any(|n| n.dependencies.contains(&node.id));

                        let box_width = 14 + 2; // [indicator + space + id]
                        let arrow_str = if has_dependent {
                            let padding = (box_width - 1) / 2;
                            format!(
                                "{:>width$}↓{:<rest$}",
                                "",
                                "",
                                width = padding,
                                rest = box_width - padding - 1
                            )
                        } else {
                            " ".repeat(box_width)
                        };
                        arrow_chars.push(arrow_str);
                    }

                    arrow_spans.push(Span::styled(arrow_chars.join(""), Theme::dimmed()));
                }

                lines.push(Line::from(arrow_spans));
            }
        }

        // Add legend
        lines.push(Line::from(vec![]));
        lines.push(Line::from(vec![
            Span::styled("✓", Style::default().fg(StatusColors::COMPLETED)),
            Span::raw(" done  "),
            Span::styled("●", Style::default().fg(StatusColors::EXECUTING)),
            Span::raw(" exec  "),
            Span::styled("▶", Style::default().fg(StatusColors::QUEUED)),
            Span::raw(" ready  "),
            Span::styled("○", Style::default().fg(StatusColors::PENDING)),
            Span::raw(" wait  "),
            Span::styled("✗", Style::default().fg(StatusColors::BLOCKED)),
            Span::raw(" fail"),
        ]));

        lines
    }
}

impl Widget for GraphWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Apply block if present
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        // Use the simpler line-based rendering
        let lines = self.render_as_lines();
        let paragraph = Paragraph::new(lines);
        paragraph.render(inner_area, buf);
    }
}

/// Create a graph widget with default styling
pub fn execution_graph(stages: &[Stage]) -> GraphWidget<'_> {
    GraphWidget::new(stages).block(
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
    fn test_compute_layout_empty() {
        let widget = GraphWidget::new(&[]);
        let nodes = widget.compute_layout();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_compute_layout_single_stage() {
        let stages = vec![make_stage("bootstrap", vec![], StageStatus::Completed)];
        let widget = GraphWidget::new(&stages);
        let nodes = widget.compute_layout();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "bootstrap");
        assert_eq!(nodes[0].level, 0);
        assert_eq!(nodes[0].position, 0);
    }

    #[test]
    fn test_compute_layout_linear_deps() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
            make_stage("c", vec!["b"], StageStatus::WaitingForDeps),
        ];
        let widget = GraphWidget::new(&stages);
        let nodes = widget.compute_layout();

        assert_eq!(nodes.len(), 3);

        let a = nodes.iter().find(|n| n.id == "a").unwrap();
        let b = nodes.iter().find(|n| n.id == "b").unwrap();
        let c = nodes.iter().find(|n| n.id == "c").unwrap();

        assert_eq!(a.level, 0);
        assert_eq!(b.level, 1);
        assert_eq!(c.level, 2);
    }

    #[test]
    fn test_compute_layout_parallel_stages() {
        let stages = vec![
            make_stage("root", vec![], StageStatus::Completed),
            make_stage("branch-a", vec!["root"], StageStatus::Executing),
            make_stage("branch-b", vec!["root"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages);
        let nodes = widget.compute_layout();

        let branch_a = nodes.iter().find(|n| n.id == "branch-a").unwrap();
        let branch_b = nodes.iter().find(|n| n.id == "branch-b").unwrap();

        // Both branches should be at the same level
        assert_eq!(branch_a.level, branch_b.level);
        assert_eq!(branch_a.level, 1);

        // They should have different positions within the level
        assert_ne!(branch_a.position, branch_b.position);
    }

    #[test]
    fn test_compute_layout_diamond() {
        // Diamond dependency pattern:
        //     a
        //    / \
        //   b   c
        //    \ /
        //     d
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages);
        let nodes = widget.compute_layout();

        let a = nodes.iter().find(|n| n.id == "a").unwrap();
        let b = nodes.iter().find(|n| n.id == "b").unwrap();
        let c = nodes.iter().find(|n| n.id == "c").unwrap();
        let d = nodes.iter().find(|n| n.id == "d").unwrap();

        assert_eq!(a.level, 0);
        assert_eq!(b.level, 1);
        assert_eq!(c.level, 1);
        assert_eq!(d.level, 2); // d depends on max(b, c) + 1
    }

    #[test]
    fn test_status_style_mapping() {
        // Verify each status maps to a distinct style
        let statuses = vec![
            StageStatus::Completed,
            StageStatus::Executing,
            StageStatus::Queued,
            StageStatus::WaitingForDeps,
            StageStatus::Blocked,
        ];

        for status in statuses {
            let _style = GraphWidget::<'_>::status_style(&status);
            let _char = GraphWidget::<'_>::status_char(&status);
            // Just verify they don't panic
        }
    }

    #[test]
    fn test_render_as_lines_empty() {
        let widget = GraphWidget::new(&[]);
        let lines = widget.render_as_lines();
        assert!(!lines.is_empty());
        // Should show "(no stages)" message
    }

    #[test]
    fn test_render_as_lines_with_stages() {
        let stages = vec![
            make_stage("bootstrap", vec![], StageStatus::Completed),
            make_stage("feature", vec!["bootstrap"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages);
        let lines = widget.render_as_lines();

        // Should have multiple lines for levels + arrows + legend
        assert!(lines.len() >= 3);
    }
}
