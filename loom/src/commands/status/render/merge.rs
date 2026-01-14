//! Merge status widget

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::MergeSummary;

/// Render merge status with CLI hints
pub fn render_merge_status<W: Write>(w: &mut W, merge: &MergeSummary) -> std::io::Result<()> {
    let total = merge.merged.len() + merge.pending.len() + merge.conflicts.len();
    if total == 0 {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "Merge Status".bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    // Merged stages
    if !merge.merged.is_empty() {
        writeln!(w, "  {} {} merged", "✓".green(), merge.merged.len())?;
    }

    // Pending stages
    if !merge.pending.is_empty() {
        writeln!(
            w,
            "  {} {} pending merge:",
            "○".yellow(),
            merge.pending.len()
        )?;
        for stage_id in &merge.pending {
            writeln!(w, "    {} {}", "→".dimmed(), stage_id)?;
        }
        writeln!(w, "    {}", "Run: loom merge <stage-id>".dimmed())?;
    }

    // Conflicts
    if !merge.conflicts.is_empty() {
        writeln!(
            w,
            "  {} {} with conflicts:",
            "✗".red().bold(),
            merge.conflicts.len()
        )?;
        for stage_id in &merge.conflicts {
            writeln!(w, "    {} {}", "⚡".red(), stage_id)?;
        }
        writeln!(
            w,
            "    {}",
            "Run: loom merge <stage-id> to start resolution".dimmed()
        )?;
    }

    Ok(())
}
