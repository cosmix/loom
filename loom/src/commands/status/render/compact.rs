//! Compact single-line output for scripting

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StatusData;
use crate::orchestrator::{context_health, ContextHealth};

/// Render single-line compact status (for scripting/monitoring)
/// Format: [4/12] ●2 ○6 ✗1 ⟳1 | ctx:100000/150000 | conflicts:0
pub fn render_compact<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    let progress = &data.progress;

    // Plan name prefix
    if let Some(ref name) = data.plan_name {
        write!(w, "{} ", name.bold())?;
    }

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

    // Review count
    let review_count = data
        .stages
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                crate::models::stage::StageStatus::NeedsHumanReview
            )
        })
        .count();
    if review_count > 0 {
        write!(
            w,
            " {}",
            format!("⏸{review_count}").color(colored::Color::Magenta)
        )?;
    }

    // Largest resident context reading.
    let max_context = data
        .stages
        .iter()
        .filter_map(|stage| stage.context_tokens.zip(stage.context_ceiling_tokens))
        .max_by_key(|(tokens, _)| *tokens);
    if let Some((tokens, ceiling)) = max_context {
        let ctx_str = format!("{tokens}/{ceiling}");
        let color = match context_health(tokens, ceiling) {
            ContextHealth::Green => colored::Color::Green,
            ContextHealth::Yellow => colored::Color::Yellow,
            ContextHealth::Red => colored::Color::Red,
        };
        let colored = ctx_str.color(color);
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
