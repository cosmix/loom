//! Sessions table widget

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::SessionSummary;
use crate::models::constants::display::{CONTEXT_HEALTHY_PCT, CONTEXT_WARNING_PCT};

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
    let colored_ctx = if ctx_pct * 100.0 >= CONTEXT_WARNING_PCT {
        ctx_str.red()
    } else if ctx_pct * 100.0 >= CONTEXT_HEALTHY_PCT {
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

/// Truncate string to max characters (UTF-8 safe)
fn truncate(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 8), "hello w…");
    }

    #[test]
    fn test_truncate_utf8_emoji() {
        // Emoji are 4 bytes each
        let input = "🎉🎊🎁🎈🎂";
        let result = truncate(input, 4);
        assert_eq!(result, "🎉🎊🎁…");
    }

    #[test]
    fn test_truncate_utf8_cjk() {
        let input = "你好世界测试";
        let result = truncate(input, 4);
        assert_eq!(result, "你好世…");
    }

    #[test]
    fn test_render_mini_bar() {
        assert_eq!(render_mini_bar(0.5, 8), "████░░░░");
        assert_eq!(render_mini_bar(1.0, 4), "████");
        assert_eq!(render_mini_bar(0.0, 4), "░░░░");
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(90), "1m");
        assert_eq!(format_uptime(3700), "1h");
    }
}
