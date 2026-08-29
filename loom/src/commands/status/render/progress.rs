//! Progress bar widget showing stage counts

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::ProgressSummary;
use crate::orchestrator::{context_health, ContextHealth};

/// Render progress bar with stage counts.
///
/// Indented to align with the rest of the status dashboard sections.
/// Shows: `   [████████░░░░░░░░]   5 / 12 stages          2 executing   3 blocked`
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

    // Indented to match the rest of the dashboard. Wide gap between bar and
    // counts so they don't visually run together.
    write!(
        w,
        "   [{}]   {} / {} stages",
        colored_bar, progress.completed, progress.total
    )?;

    if progress.executing > 0 {
        write!(w, "          {} {}", progress.executing, "executing".blue())?;
    }

    if progress.blocked > 0 {
        write!(w, "   {} {}", progress.blocked, "blocked".red().bold())?;
    }

    writeln!(w)?;
    Ok(())
}

/// Render resident context tokens against their resolved ceiling.
pub fn render_context_bar(tokens: u32, ceiling: u32, width: usize) -> String {
    let filled = if ceiling == 0 {
        0
    } else {
        ((tokens as f64 / ceiling as f64) * width as f64).min(width as f64) as usize
    };
    let fill = match context_health(tokens, ceiling) {
        ContextHealth::Green => '░',
        ContextHealth::Yellow => '▓',
        ContextHealth::Red => '█',
    };

    let bar: String = (0..width)
        .map(|i| if i < filled { fill } else { '·' })
        .collect();

    format!("[{bar}] {tokens}/{ceiling}")
}
