//! Merge status widget

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::MergeSummary;

/// Render merge status (only pending/conflicts that need action)
pub fn render_merge_status<W: Write>(w: &mut W, merge: &MergeSummary) -> std::io::Result<()> {
    if merge.pending.is_empty() && merge.conflicts.is_empty() {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "Pending Merges".bold())?;

    // Pending stages
    for stage_id in &merge.pending {
        writeln!(w, "  {} {}", "○".yellow(), stage_id)?;
    }

    // Conflicts
    for stage_id in &merge.conflicts {
        writeln!(w, "  {} {} {}", "⚡".red(), stage_id, "conflict".red())?;
    }

    writeln!(w, "  {}", "Run: loom stage retry-merge <stage-id>".dimmed())?;

    Ok(())
}
