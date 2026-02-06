//! Sessions table widget

use colored::Colorize;
use std::io::Write;

use crate::commands::common::truncate;
use crate::commands::status::data::SessionSummary;
use crate::utils::{context_pct_terminal_color, format_elapsed};

/// Render sessions table with context bars
pub fn render_sessions<W: Write>(w: &mut W, sessions: &[SessionSummary]) -> std::io::Result<()> {
    if sessions.is_empty() {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "Active Sessions".bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    // Header
    writeln!(
        w,
        "  {:12} {:16} {:>6} {:>12} {:>8}",
        "SESSION".dimmed(),
        "STAGE".dimmed(),
        "PID".dimmed(),
        "CONTEXT".dimmed(),
        "UPTIME".dimmed()
    )?;

    for session in sessions {
        render_session_row(w, session)?;
    }

    Ok(())
}

fn render_session_row<W: Write>(w: &mut W, session: &SessionSummary) -> std::io::Result<()> {
    let id = truncate(&session.id, 12);
    let stage = session
        .stage_id
        .as_deref()
        .map(|s| truncate(s, 16))
        .unwrap_or_else(|| "-".to_string());
    let pid = session
        .pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string());

    // Context bar
    let ctx_pct = session.context_tokens as f32 / session.context_limit.max(1) as f32;
    let ctx_bar = render_mini_bar(ctx_pct, 8);
    let ctx_str = format!("{} {:>3.0}%", ctx_bar, ctx_pct * 100.0);
    let color = context_pct_terminal_color(ctx_pct * 100.0);
    let colored_ctx = ctx_str.color(color);

    // Uptime
    let uptime = format_elapsed(session.uptime_secs);

    // Alive indicator
    let alive_indicator = if session.is_alive { "" } else { " ✗" };

    writeln!(
        w,
        "  {:12} {:16} {:>6} {} {:>8}{}",
        id.dimmed(),
        stage,
        pid,
        colored_ctx,
        uptime.dimmed(),
        alive_indicator.red()
    )?;

    Ok(())
}

fn render_mini_bar(pct: f32, width: usize) -> String {
    let filled = (pct * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mini_bar() {
        assert_eq!(render_mini_bar(0.5, 8), "████░░░░");
        assert_eq!(render_mini_bar(1.0, 4), "████");
        assert_eq!(render_mini_bar(0.0, 4), "░░░░");
    }
}
