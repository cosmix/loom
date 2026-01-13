use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::theme::Theme;
use crate::daemon::StageInfo;
use crate::models::stage::StageStatus;

/// Unicode block characters for progress bars
const BLOCKS: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Create a progress bar using Unicode block characters
///
/// # Arguments
/// * `pct` - Progress percentage (0.0 to 1.0)
/// * `width` - Total width in characters
/// * `style` - Style for filled portion
pub fn progress_bar(pct: f32, width: usize, _style: Style) -> String {
    let pct = pct.clamp(0.0, 1.0);
    let filled_width = pct * width as f32;
    let full_blocks = filled_width.floor() as usize;
    let partial_idx = ((filled_width - full_blocks as f32) * 8.0).round() as usize;

    let mut bar = String::with_capacity(width);

    // Full blocks
    for _ in 0..full_blocks {
        bar.push(BLOCKS[8]);
    }

    // Partial block
    if full_blocks < width && partial_idx > 0 {
        bar.push(BLOCKS[partial_idx]);
    }

    // Empty space
    let current_len = bar.chars().count();
    for _ in current_len..width {
        bar.push(BLOCKS[0]);
    }

    bar
}

/// Create a mini context bar for tables (compact version)
///
/// # Arguments
/// * `pct` - Context percentage (0.0 to 1.0)
/// * `width` - Bar width (default: 8)
pub fn context_bar(pct: f32, width: usize) -> Line<'static> {
    let style = Theme::context_style(pct);
    let bar = progress_bar(pct, width, style);
    let pct_str = format!("{:3.0}%", pct * 100.0);

    Line::from(vec![
        Span::styled(bar, style),
        Span::raw(" "),
        Span::styled(pct_str, style),
    ])
}

/// Get status indicator symbol with color
///
/// Returns a styled Span with the appropriate Unicode symbol
pub fn status_indicator(status: &StageStatus) -> Span<'static> {
    match status {
        StageStatus::Completed => Span::styled("✓", Theme::status_completed()),
        StageStatus::Executing => Span::styled("●", Theme::status_executing()),
        StageStatus::Queued => Span::styled("▶", Theme::status_queued()),
        StageStatus::WaitingForDeps => Span::styled("○", Theme::status_pending()),
        StageStatus::Blocked => Span::styled("✗", Theme::status_blocked()),
        StageStatus::NeedsHandoff => Span::styled("⟳", Theme::status_warning()),
        StageStatus::WaitingForInput => Span::styled("?", Theme::status_warning()),
        StageStatus::Skipped => Span::styled("⊘", Theme::dimmed()),
        StageStatus::MergeConflict => Span::styled("⚡", Theme::status_blocked()),
        StageStatus::CompletedWithFailures => Span::styled("⚠", Theme::status_warning()),
        StageStatus::MergeBlocked => Span::styled("⊗", Theme::status_blocked()),
    }
}

/// Get status text description
pub fn status_text(status: &StageStatus) -> &'static str {
    match status {
        StageStatus::Completed => "Completed",
        StageStatus::Executing => "Executing",
        StageStatus::Queued => "Queued",
        StageStatus::WaitingForDeps => "Waiting",
        StageStatus::Blocked => "Blocked",
        StageStatus::NeedsHandoff => "Handoff",
        StageStatus::WaitingForInput => "Input",
        StageStatus::Skipped => "Skipped",
        StageStatus::MergeConflict => "Conflict",
        StageStatus::CompletedWithFailures => "Failed",
        StageStatus::MergeBlocked => "MergeErr",
    }
}

/// Merged status indicator
pub fn merged_indicator(merged: bool) -> Span<'static> {
    if merged {
        Span::styled("✓", Theme::status_merged())
    } else {
        Span::styled("○", Theme::dimmed())
    }
}

/// Render execution graph as ASCII DAG
///
/// Creates a compact visualization showing stage dependencies and status.
/// Format: [node] --> [node,node] --> [node]
///
/// Each node shows: status_icon + abbreviated_id
pub fn render_execution_graph<'a>(
    executing: &[StageInfo],
    pending: &[String],
    completed: &[String],
    blocked: &[String],
) -> Vec<Line<'a>> {
    // Group stages by their depth (based on dependencies)
    let mut depth_map: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    // Calculate depths - stages with dependencies from executing stages
    for stage in executing {
        if stage.dependencies.is_empty() {
            depth_map.insert(&stage.id, 0);
        } else {
            let max_dep_depth = stage
                .dependencies
                .iter()
                .filter_map(|d| depth_map.get(d.as_str()))
                .max()
                .copied()
                .unwrap_or(0);
            depth_map.insert(&stage.id, max_dep_depth + 1);
        }
    }

    // For stages without dependency info, assign them estimated depths
    // Completed stages are likely earlier, pending stages are likely later
    let base_depth = depth_map.values().max().copied().unwrap_or(0);
    for id in completed {
        depth_map.entry(id.as_str()).or_insert(0);
    }
    for id in pending {
        depth_map.entry(id.as_str()).or_insert(base_depth + 1);
    }
    for id in blocked {
        depth_map.entry(id.as_str()).or_insert(base_depth);
    }

    // Group by depth
    let mut by_depth: std::collections::BTreeMap<usize, Vec<&str>> =
        std::collections::BTreeMap::new();
    for (id, depth) in &depth_map {
        by_depth.entry(*depth).or_default().push(*id);
    }

    // Build graph line
    let mut graph_spans: Vec<Span<'a>> = Vec::new();

    for (i, (_, ids)) in by_depth.iter().enumerate() {
        if i > 0 {
            graph_spans.push(Span::styled(" → ", Theme::graph_edge()));
        }

        graph_spans.push(Span::raw("["));

        for (j, id) in ids.iter().enumerate() {
            if j > 0 {
                graph_spans.push(Span::raw(","));
            }

            // Determine status for this ID
            let (icon, style) = if executing.iter().any(|s| s.id == *id) {
                ("●", Theme::graph_node_active())
            } else if completed.contains(&id.to_string()) {
                ("✓", Theme::graph_node_done())
            } else if blocked.contains(&id.to_string()) {
                ("✗", Theme::status_blocked())
            } else {
                ("○", Theme::graph_node_pending())
            };

            // Abbreviate ID for display
            let abbrev = abbreviate_id(id, 12);
            graph_spans.push(Span::styled(format!("{icon}{abbrev}"), style));
        }

        graph_spans.push(Span::raw("]"));
    }

    if graph_spans.is_empty() {
        return vec![Line::from(Span::styled("No stages", Theme::dimmed()))];
    }

    vec![Line::from(graph_spans)]
}

/// Abbreviate a stage ID for compact display
fn abbreviate_id(id: &str, max_len: usize) -> String {
    if id.len() <= max_len {
        id.to_string()
    } else {
        format!("{}…", &id[..max_len - 1])
    }
}
