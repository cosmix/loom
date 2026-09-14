//! Compact single-line output for scripting

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StatusData;
use crate::orchestrator::{context_health, ContextHealth};

/// Render single-line compact status (for scripting/monitoring)
/// Format: [4/12] ●2 ○6 ✗1 ⟳1 | ctx:100000/150000 | conflicts:0
pub fn render_compact<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    let progress = &data.progress;
    if let Some(ref name) = data.plan_name {
        write!(w, "{} ", name.bold())?;
    }
    write!(w, "[{}/{}]", progress.completed, progress.total)?;
    write!(w, " ●{}", progress.executing)?;
    write!(w, " ○{}", progress.pending)?;
    if progress.blocked > 0 {
        write!(w, " {}", format!("✗{}", progress.blocked).red())?;
    }

    render_attention_counts(w, data)?;
    render_context(w, data)?;
    let conflicts = data.merge.conflicts.len();
    if conflicts > 0 {
        write!(w, " | {}", format!("conflicts:{conflicts}").red())?;
    }
    writeln!(w)?;
    render_completion_blockers(w, data)
}

fn render_attention_counts<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    let handoff_count = data
        .stages
        .iter()
        .filter(|s| matches!(s.status, crate::models::stage::StageStatus::NeedsHandoff))
        .count();
    if handoff_count > 0 {
        write!(w, " ⟳{handoff_count}")?;
    }
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
    Ok(())
}

fn render_context<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
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
    Ok(())
}

fn render_completion_blockers<W: Write>(w: &mut W, data: &StatusData) -> std::io::Result<()> {
    for stage in &data.stages {
        let Some(blocker) = stage.completion_blocker.as_ref() else {
            continue;
        };
        let line = format!(
            "{}: {} - next: {}",
            stage.id,
            blocker.activity_text(),
            blocker.next_action
        );
        writeln!(w, "{}", bounded_line(&line))?;
    }
    Ok(())
}

fn bounded_line(line: &str) -> String {
    const MAX_CHARS: usize = 160;
    if line.chars().count() <= MAX_CHARS {
        return line.to_owned();
    }
    let mut bounded = line.chars().take(MAX_CHARS - 1).collect::<String>();
    bounded.push('…');
    bounded
}
