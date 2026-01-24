//! Completion screen renderer for orchestration results.
//!
//! Displays the final status of all stages after orchestration completes,
//! including timing information and success/failure summary.

use std::io::{self, Write};

use colored::Colorize;

use crate::daemon::{CompletionSummary, StageCompletionInfo};
use crate::models::stage::StageStatus;

/// Truncate string to max characters (UTF-8 safe)
fn truncate_chars(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max.saturating_sub(3)).collect::<String>())
    }
}

/// Status indicator character for display
fn status_indicator(status: &StageStatus) -> &'static str {
    match status {
        StageStatus::Completed => "\u{2713}",             // ✓
        StageStatus::Skipped => "\u{2298}",               // ⊘
        StageStatus::Blocked => "\u{2717}",               // ✗
        StageStatus::MergeConflict => "\u{26A1}",         // ⚡
        StageStatus::CompletedWithFailures => "\u{26A0}", // ⚠
        StageStatus::MergeBlocked => "\u{2297}",          // ⊗
        _ => "\u{25CB}",                                  // ○
    }
}

/// Format elapsed time in human-readable format
fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Get stage duration from duration_secs field
fn stage_duration(stage: &StageCompletionInfo) -> Option<i64> {
    stage.duration_secs
}

/// Render the completion screen to stdout
pub fn render_completion_screen(summary: &CompletionSummary) {
    // Clear screen
    print!("\x1B[2J\x1B[1;1H");

    // Header
    println!();
    let success = summary.failure_count == 0;
    let header = if success {
        format!(
            "{} {}",
            "\u{2713}".green().bold(),
            "Orchestration Complete".green().bold()
        )
    } else {
        format!(
            "{} {}",
            "\u{2717}".red().bold(),
            "Orchestration Complete (with failures)".red().bold()
        )
    };
    println!("{header}");
    println!("{}", "\u{2550}".repeat(50));

    // Summary line
    let total_time = format_elapsed(summary.total_duration_secs);
    println!(
        "\n{} {} | {} {} | {} {}",
        "Total:".bold(),
        total_time,
        "\u{2713}".green(),
        summary.success_count,
        "\u{2717}".red(),
        summary.failure_count
    );

    // Stage table header
    println!("\n{}", "Stage Results".bold());
    println!("{}", "\u{2500}".repeat(50));
    println!("{:2} {:30} {:10} {:>8}", "", "Stage", "Status", "Duration");
    println!("{}", "\u{2500}".repeat(50));

    // Sort stages by completion (completed first, then by id)
    let mut sorted_stages = summary.stages.clone();
    sorted_stages.sort_by(|a, b| match (&a.duration_secs, &b.duration_secs) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });

    // Stage rows
    for stage in &sorted_stages {
        let icon = status_indicator(&stage.status);
        let duration = stage_duration(stage)
            .map(format_elapsed)
            .unwrap_or_else(|| "-".to_string());

        let status_str = match stage.status {
            StageStatus::Completed => "Completed".green(),
            StageStatus::Skipped => "Skipped".dimmed(),
            StageStatus::Blocked => "Blocked".red(),
            StageStatus::MergeConflict => "Conflict".yellow(),
            StageStatus::CompletedWithFailures => "Failed".red(),
            StageStatus::MergeBlocked => "MergeBlk".red(),
            _ => stage.status.to_string().dimmed(),
        };

        let icon_colored = match stage.status {
            StageStatus::Completed => icon.green(),
            StageStatus::Skipped => icon.dimmed(),
            StageStatus::Blocked
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked => icon.red(),
            StageStatus::MergeConflict => icon.yellow(),
            _ => icon.dimmed(),
        };

        // Truncate stage id if too long (UTF-8 safe)
        let id_display = truncate_chars(&stage.id, 28);

        println!("{icon_colored:2} {id_display:30} {status_str:10} {duration:>8}");
    }

    println!("{}", "\u{2500}".repeat(50));

    // Footer
    println!("\n{}", "Press q to exit".dimmed());
    println!();

    io::stdout().flush().ok();
}

/// Render the completion screen for TUI (ratatui)
pub fn render_completion_lines(summary: &CompletionSummary) -> Vec<String> {
    let mut lines = Vec::new();

    // Header
    lines.push(String::new());
    let success = summary.failure_count == 0;
    let header = if success {
        "\u{2713} Orchestration Complete".to_string()
    } else {
        "\u{2717} Orchestration Complete (with failures)".to_string()
    };
    lines.push(header);
    lines.push("\u{2550}".repeat(50));

    // Summary
    let total_time = format_elapsed(summary.total_duration_secs);
    lines.push(format!(
        "Total: {} | \u{2713} {} | \u{2717} {}",
        total_time, summary.success_count, summary.failure_count
    ));

    // Stage table
    lines.push(String::new());
    lines.push("Stage Results".to_string());
    lines.push("\u{2500}".repeat(50));
    lines.push(format!(
        "{:2} {:30} {:10} {:>8}",
        "", "Stage", "Status", "Duration"
    ));
    lines.push("\u{2500}".repeat(50));

    let mut sorted_stages = summary.stages.clone();
    sorted_stages.sort_by(|a, b| match (&a.duration_secs, &b.duration_secs) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });

    for stage in &sorted_stages {
        let icon = status_indicator(&stage.status);
        let duration = stage_duration(stage)
            .map(format_elapsed)
            .unwrap_or_else(|| "-".to_string());

        let status_str = match stage.status {
            StageStatus::Completed => "Completed",
            StageStatus::Skipped => "Skipped",
            StageStatus::Blocked => "Blocked",
            StageStatus::MergeConflict => "Conflict",
            StageStatus::CompletedWithFailures => "Failed",
            StageStatus::MergeBlocked => "MergeBlk",
            _ => "Other",
        };

        // Truncate stage id if too long (UTF-8 safe)
        let id_display = truncate_chars(&stage.id, 28);

        lines.push(format!(
            "{icon:2} {id_display:30} {status_str:10} {duration:>8}"
        ));
    }

    lines.push("\u{2500}".repeat(50));
    lines.push(String::new());
    lines.push("Press q to exit".to_string());

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stage_info(id: &str, status: StageStatus, completed: bool) -> StageCompletionInfo {
        StageCompletionInfo {
            id: id.to_string(),
            name: id.to_string(),
            status,
            duration_secs: if completed { Some(120) } else { None },
            merged: completed,
            dependencies: vec![],
        }
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3661), "1h1m");
    }

    #[test]
    fn test_status_indicator() {
        assert_eq!(status_indicator(&StageStatus::Completed), "\u{2713}");
        assert_eq!(status_indicator(&StageStatus::Blocked), "\u{2717}");
        assert_eq!(status_indicator(&StageStatus::Skipped), "\u{2298}");
    }

    #[test]
    fn test_render_completion_lines() {
        let summary = CompletionSummary {
            stages: vec![
                make_stage_info("bootstrap", StageStatus::Completed, true),
                make_stage_info("implement", StageStatus::Completed, true),
            ],
            total_duration_secs: 180,
            success_count: 2,
            failure_count: 0,
            plan_path: "doc/plans/PLAN-test.md".to_string(),
        };

        let lines = render_completion_lines(&summary);
        assert!(lines.iter().any(|l| l.contains("Orchestration Complete")));
        assert!(lines.iter().any(|l| l.contains("bootstrap")));
        assert!(lines.iter().any(|l| l.contains("implement")));
        assert!(lines.iter().any(|l| l.contains("3m0s"))); // 180 seconds
    }

    #[test]
    fn test_render_completion_lines_with_failures() {
        let summary = CompletionSummary {
            stages: vec![
                make_stage_info("bootstrap", StageStatus::Completed, true),
                make_stage_info("failing", StageStatus::Blocked, false),
            ],
            total_duration_secs: 60,
            success_count: 1,
            failure_count: 1,
            plan_path: "doc/plans/PLAN-test.md".to_string(),
        };

        let lines = render_completion_lines(&summary);
        assert!(lines.iter().any(|l| l.contains("with failures")));
        assert!(lines.iter().any(|l| l.contains("failing")));
    }

    #[test]
    fn test_truncate_chars_short() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_chars_exact() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_chars_long() {
        // 28 chars max: truncate to 25 + "..."
        let long_id = "a".repeat(30);
        let result = truncate_chars(&long_id, 28);
        assert_eq!(result.len(), 28);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_chars_utf8_emoji() {
        // Emoji are 4 bytes each, but only 1 char
        let input = "🎉🎊🎁🎈🎂🎄🎅🎆"; // 8 emoji = 8 chars
        let result = truncate_chars(input, 6);
        // Should be 3 emoji + "..."
        assert_eq!(result, "🎉🎊🎁...");
    }

    #[test]
    fn test_truncate_chars_utf8_cjk() {
        // CJK are 3 bytes each
        let input = "你好世界测试安全"; // 8 CJK chars
        let result = truncate_chars(input, 6);
        assert_eq!(result, "你好世...");
    }
}
