//! Execution graph widget showing stage dependencies

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::{StageSummary, StatusData};
use crate::models::stage::StageStatus;

/// Render execution graph with status indicators
pub fn render_graph<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    writeln!(w, "{}", "Execution Graph".bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    for stage in &data.stages {
        render_stage_line(w, stage)?;
    }

    // Legend
    writeln!(w)?;
    write!(w, "  {} ", "Legend:".dimmed())?;
    write!(w, "{} ", "✓".green())?;
    write!(w, "done  ")?;
    write!(w, "{} ", "●".blue())?;
    write!(w, "exec  ")?;
    write!(w, "{} ", "○".dimmed())?;
    write!(w, "wait  ")?;
    write!(w, "{} ", "✗".red())?;
    writeln!(w, "blocked")?;

    Ok(())
}

fn render_stage_line<W: Write>(w: &mut W, stage: &StageSummary) -> std::io::Result<()> {
    // Status indicator
    let indicator = match &stage.status {
        StageStatus::Completed => "✓".green(),
        StageStatus::Executing => "●".blue().bold(),
        StageStatus::Queued => "▶".cyan(),
        StageStatus::WaitingForDeps => "○".dimmed(),
        StageStatus::Blocked => "✗".red().bold(),
        StageStatus::NeedsHandoff => "⟳".yellow(),
        StageStatus::MergeConflict => "⚡".red(),
        StageStatus::WaitingForInput => "?".yellow(),
        StageStatus::Skipped => "⊘".dimmed(),
        StageStatus::CompletedWithFailures => "⚠".yellow(),
        StageStatus::MergeBlocked => "⊗".red(),
    };

    // Stage name
    let name = match &stage.status {
        StageStatus::Executing => stage.name.blue().bold().to_string(),
        StageStatus::Blocked | StageStatus::MergeConflict | StageStatus::MergeBlocked => {
            stage.name.red().to_string()
        }
        StageStatus::Completed => stage.name.dimmed().to_string(),
        _ => stage.name.clone(),
    };

    write!(w, "  {indicator} {name}")?;

    // Context percentage if executing
    if let Some(ctx_pct) = stage.context_pct {
        let ctx_str = format!(" [{:.0}%]", ctx_pct * 100.0);
        let colored_ctx = if ctx_pct > 0.75 {
            ctx_str.red()
        } else if ctx_pct > 0.6 {
            ctx_str.yellow()
        } else {
            ctx_str.dimmed()
        };
        write!(w, "{colored_ctx}")?;
    }

    // Elapsed time
    if let Some(secs) = stage.elapsed_secs {
        let elapsed = format_elapsed(secs);
        write!(w, " {}", elapsed.dimmed())?;
    }

    // Base branch for executing stages
    if matches!(stage.status, StageStatus::Executing | StageStatus::Queued) {
        if let Some(ref base) = stage.base_branch {
            write!(w, " {}", format!("({base})").dimmed())?;
        }
    }

    writeln!(w)?;
    Ok(())
}

fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
