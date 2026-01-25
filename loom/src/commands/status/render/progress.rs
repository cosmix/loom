//! Progress bar widget showing stage counts

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::ProgressSummary;

/// Render progress bar with stage counts
/// Shows: [████████░░░░░░░░] 5/12 stages | 2 executing | 3 blocked (!)
pub fn render_progress<W: Write>(w: &mut W, progress: &ProgressSummary) -> std::io::Result<()> {
    let pct = if progress.total > 0 {
        progress.completed as f32 / progress.total as f32
    } else {
        0.0
    };

    // Build progress bar (width 20)
    let width = 20;
    let filled = (pct * width as f32).round() as usize;
    let empty = width - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    // Color the bar based on progress
    let colored_bar = if progress.blocked > 0 {
        bar.yellow()
    } else if pct >= 1.0 {
        bar.green()
    } else {
        bar.blue()
    };

    // Build status line
    write!(
        w,
        "[{}] {}/{} stages",
        colored_bar, progress.completed, progress.total
    )?;

    if progress.executing > 0 {
        write!(w, " | {} {}", progress.executing, "executing".blue())?;
    }

    if progress.blocked > 0 {
        write!(w, " | {} {} (!)", progress.blocked, "blocked".red().bold())?;
    }

    writeln!(w)?;
    Ok(())
}

/// Render context budget bar with threshold marker
/// Shows current usage with a marker at the budget threshold
pub fn render_context_bar(usage_pct: f32, budget_pct: Option<f32>, width: usize) -> String {
    let budget = budget_pct.unwrap_or(65.0);
    let filled = ((usage_pct / 100.0) * width as f32) as usize;
    let budget_marker = ((budget / 100.0) * width as f32) as usize;

    let bar: String = (0..width)
        .map(|i| {
            if i < filled {
                if usage_pct >= 65.0 {
                    '█'
                } else if usage_pct >= 50.0 {
                    '▓'
                } else {
                    '░'
                }
            } else if i == budget_marker {
                '│' // Budget threshold marker
            } else {
                '·'
            }
        })
        .collect();

    format!("[{bar}] {usage_pct:.0}%")
}
