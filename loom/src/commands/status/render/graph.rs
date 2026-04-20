//! Tree-based execution graph display for status command
//!
//! Renders stages as a vertical tree with connectors, dependency annotations,
//! and inline status details (session, failure, merge info).

use std::collections::HashMap;
use std::io::Write;

use colored::{Color, Colorize};

use crate::commands::graph::colors::color_by_index;
use crate::commands::graph::indicators::status_indicator;
use crate::commands::status::data::{StageSummary, StatusData};
use crate::models::failure::FailureType;
use crate::models::stage::{StageStatus, StageType};
use crate::plan::graph::levels;
use crate::utils::{context_pct_terminal_color, format_elapsed};

/// Compute topological level for each stage (level = max(dep_levels) + 1)
fn compute_stage_levels(stages: &[StageSummary]) -> HashMap<String, usize> {
    levels::compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies)
}

/// Compute the tree connector prefix.
///
/// Indents 3 columns per level and uses `└─ ` for the last stage at any level,
/// `├─ ` otherwise. Root-level stages get no connector. The original
/// implementation only used `└─` for the very last row in the very last level,
/// which made single-child chains render as `├──` everywhere — visually wrong.
fn compute_connector(level: usize, index_in_level: usize, level_size: usize) -> String {
    let indent = "   ".repeat(level);

    if level == 0 {
        indent
    } else if index_in_level == level_size - 1 {
        format!("{indent}└─ ")
    } else {
        format!("{indent}├─ ")
    }
}

/// Format inline dependency annotation: `  ← dep1, dep2` placed right after the
/// stage id. Colors each dep id using the shared color map. Returns an empty
/// string when the stage has no deps.
fn format_dep_annotation(deps: &[String], color_map: &HashMap<&str, Color>) -> String {
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

/// Format inline annotations for a stage (session, failure, merge, held)
fn format_stage_annotations(stage: &StageSummary) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Context percentage (only when meaningful)
    if matches!(stage.status, StageStatus::Executing) {
        if let Some(ctx_pct) = stage.context_pct {
            let pct_val = ctx_pct * 100.0;
            let ctx_str = format!("[{:.0}%]", pct_val);
            let color = context_pct_terminal_color(pct_val);
            parts.push(format!("{}", ctx_str.color(color)));
        }

        if let Some(secs) = stage.elapsed_secs {
            parts.push(format!("{}", format_elapsed(secs).dimmed()));
        }

        // Activity icon
        let activity_icon = stage.activity_status.icon();
        parts.push(activity_icon.to_string());

        // Staleness warning
        if let Some(staleness) = stage.staleness_secs {
            if staleness > 300 {
                parts.push(format!("{}", "(stale)".yellow()));
            }
        }

        // Session PID or orphaned
        if let Some(pid) = stage.pid {
            if stage.session_alive {
                parts.push(format!("{}", format!("PID {pid}").dimmed()));
            } else {
                parts.push(format!("{}", "orphaned".red()));
            }
        }
    }

    // Held indicator
    if stage.held {
        parts.push(format!("{}", "HELD".yellow()));
    }

    // Failure info for blocked stages
    if stage.status == StageStatus::Blocked {
        let max = stage.max_retries.unwrap_or(3);
        let failure_label = stage
            .failure_info
            .as_ref()
            .map(|i| match i.failure_type {
                FailureType::SessionCrash => "crash",
                FailureType::TestFailure => "test",
                FailureType::BuildFailure => "build",
                FailureType::CodeError => "code",
                FailureType::Timeout => "timeout",
                FailureType::ContextExhausted => "context",
                FailureType::UserBlocked => "user",
                FailureType::MergeConflict => "merge",
                FailureType::InfrastructureError => "infra",
                FailureType::Unknown => "error",
            })
            .unwrap_or("error");
        parts.push(format!(
            "{}",
            format!("{failure_label} ({}/{max})", stage.retry_count).red()
        ));
    }

    // Review reason for NeedsHumanReview
    if stage.status == StageStatus::NeedsHumanReview {
        if let Some(ref reason) = stage.review_reason {
            parts.push(format!("{}", reason.yellow()));
        }
    }

    // Merge status for completed stages
    if stage.status == StageStatus::Completed {
        if stage.merged {
            parts.push(format!("{}", "merged".green().dimmed()));
        } else if !matches!(stage.stage_type, StageType::Knowledge) {
            // Completed but not merged and not a knowledge stage — needs manual merge
            parts.push(format!("{}", "unmerged".yellow()));
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        let sep = format!(" {} ", "·".dimmed());
        format!("  {}", parts.join(&sep))
    }
}

/// 3-space indent applied to every dashboard row so the tree visually aligns
/// with the surrounding header / progress / legend sections.
const ROW_INDENT: &str = "   ";

/// Render execution graph with tree display
pub fn render_graph<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    if data.stages.is_empty() {
        writeln!(w, "{ROW_INDENT}{}", "(no stages found)".dimmed())?;
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

    // Count stages per level for connector logic (last stage at each level
    // gets `└─`; others get `├─`).
    let mut level_counts: HashMap<usize, usize> = HashMap::new();
    let mut level_indices: HashMap<usize, usize> = HashMap::new();
    for stage in &sorted_stages {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        *level_counts.entry(level).or_insert(0) += 1;
    }

    for (global_index, stage) in sorted_stages.iter().enumerate() {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        let index_in_level = *level_indices.entry(level).or_insert(0);
        let level_size = level_counts.get(&level).copied().unwrap_or(1);

        let connector = compute_connector(level, index_in_level, level_size);
        let indicator = status_indicator(&stage.status);
        let deps = format_dep_annotation(&stage.dependencies, &color_map);
        let color = color_by_index(global_index);
        let colored_id = stage.id.color(color);
        let model_tag = format!(" {}", format!("[{}]", stage.model).dimmed());
        let annotations = format_stage_annotations(stage);

        // Layout: <indent> <connector> <indicator>  <id> <model> <deps> <annotations>
        // Two spaces between indicator and id give room to breathe; deps and
        // annotations sit inline (no fragile column padding).
        writeln!(
            w,
            "{ROW_INDENT}{connector}{indicator}  {colored_id}{model_tag}{deps}{annotations}"
        )?;

        // For completed-but-not-merged non-knowledge stages, show a merge hint.
        if stage.status == StageStatus::Completed
            && !stage.merged
            && !matches!(stage.stage_type, StageType::Knowledge)
        {
            // Indent to align under the stage id (connector width + icon + 2 spaces)
            let hint_indent = " ".repeat(connector.chars().count() + 4);
            let hint = format!("→ run: loom stage merge {}", stage.id);
            writeln!(w, "{ROW_INDENT}{hint_indent}{}", hint.yellow().dimmed())?;
        }

        // Increment index for this level
        *level_indices.get_mut(&level).unwrap() += 1;
    }

    writeln!(w)?;
    render_legend(w)?;

    Ok(())
}

/// Render the legend explaining status indicators. Items separated by a dimmed
/// middle dot and indented to match the rest of the dashboard.
fn render_legend<W: Write>(w: &mut W) -> std::io::Result<()> {
    let dot = format!(" {} ", "·".dimmed());
    let parts = [
        format!("{} {}", "✓".green(), "done"),
        format!("{} {}", "●".blue(), "exec"),
        format!("{} {}", "▶".cyan(), "ready"),
        format!("{} {}", "○".dimmed(), "wait"),
        format!("{} {}", "✗".red(), "blocked"),
        format!("{} {}", "⟳".yellow(), "handoff"),
    ];
    writeln!(w, "{ROW_INDENT}{}", parts.join(&dot))?;
    Ok(())
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
