//! Sessions table widget

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::SessionSummary;

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
    let colored_ctx = if ctx_pct > 0.75 {
        ctx_str.red()
    } else if ctx_pct > 0.6 {
        ctx_str.yellow()
    } else {
        ctx_str.normal()
    };

    // Uptime
    let uptime = format_uptime(session.uptime_secs);

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

fn format_uptime(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
