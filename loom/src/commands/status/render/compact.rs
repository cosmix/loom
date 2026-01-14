//! Compact single-line output for scripting

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StatusData;

/// Render single-line compact status (for scripting/monitoring)
/// Format: [4/12] ●2 ○6 ✗1 ⟳1 | ctx:67% | conflicts:0
pub fn render_compact<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    let progress = &data.progress;

    // Progress fraction
    write!(w, "[{}/{}]", progress.completed, progress.total)?;

    // Status counts
    write!(w, " ●{}", progress.executing)?;
    write!(w, " ○{}", progress.pending)?;

    if progress.blocked > 0 {
        write!(w, " {}", format!("✗{}", progress.blocked).red())?;
    }

    // Handoff count
    let handoff_count = data
        .stages
        .iter()
        .filter(|s| matches!(s.status, crate::models::stage::StageStatus::NeedsHandoff))
        .count();
    if handoff_count > 0 {
        write!(w, " ⟳{handoff_count}")?;
    }

    // Max context usage
    let max_ctx = data
        .stages
        .iter()
        .filter_map(|s| s.context_pct)
        .fold(0.0f32, |a, b| a.max(b));
    if max_ctx > 0.0 {
        let ctx_str = format!("{:.0}%", max_ctx * 100.0);
        let colored = if max_ctx > 0.75 {
            ctx_str.red()
        } else if max_ctx > 0.6 {
            ctx_str.yellow()
        } else {
            ctx_str.normal()
        };
        write!(w, " | ctx:{colored}")?;
    }

    // Conflict count
    let conflicts = data.merge.conflicts.len();
    if conflicts > 0 {
        write!(w, " | {}", format!("conflicts:{conflicts}").red())?;
    }

    writeln!(w)?;
    Ok(())
}
