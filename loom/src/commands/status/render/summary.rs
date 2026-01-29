//! Shared completion summary rendering for both TUI and foreground modes.

use std::collections::HashMap;
use std::io::{self, Write};

use crate::commands::status::common::levels;
use crate::daemon::{CompletionSummary, StageCompletionInfo};
use crate::models::stage::StageStatus;
use crate::utils::format_elapsed_verbose;

/// Print completion summary to stdout.
///
/// This function renders a detailed completion summary including:
/// - Overall status (success/failure)
/// - Plan path and timing information
/// - Execution graph showing stage dependencies
/// - Detailed stage table with status and duration
///
/// Used by both the TUI (after exit) and foreground mode.
pub fn print_completion_summary(summary: &CompletionSummary) {
    let success = summary.failure_count == 0;
    let status_icon = if success { "\u{2713}" } else { "\u{2717}" };
    let status_text = if success {
        "Orchestration Complete"
    } else {
        "Orchestration Complete (with failures)"
    };

    println!();
    println!("{status_icon} {status_text}");
    println!("══════════════════════════════════════════════════");
    println!();

    // Show plan path
    println!("Plan: {}", summary.plan_path);

    let total_time = format_elapsed_verbose(summary.total_duration_secs);
    let success_count = summary.success_count;
    let failure = summary.failure_count;
    println!("Total: {total_time} | \u{2713} {success_count} | \u{2717} {failure}");
    println!();

    // Build and display execution graph
    if !summary.stages.is_empty() {
        println!("Execution Graph:");
        print_execution_graph(&summary.stages);
        println!();
    }

    // Print detailed stage table
    println!("Stages:");
    for stage in &summary.stages {
        let icon = match stage.status {
            StageStatus::Completed => "\u{2713}",
            StageStatus::Skipped => "\u{2298}",
            StageStatus::Blocked => "\u{2717}",
            StageStatus::MergeConflict => "\u{26A1}",
            StageStatus::CompletedWithFailures => "\u{26A0}",
            StageStatus::MergeBlocked => "\u{2297}",
            _ => "\u{25CB}",
        };
        let status_text = match stage.status {
            StageStatus::Completed => "Completed",
            StageStatus::Skipped => "Skipped",
            StageStatus::Blocked => "Blocked",
            StageStatus::MergeConflict => "Conflict",
            StageStatus::CompletedWithFailures => "Failed",
            StageStatus::MergeBlocked => "MergeBlk",
            _ => "Other",
        };
        let duration = stage
            .duration_secs
            .map(format_elapsed_verbose)
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  {} {:<30} {:<12} {:>8}",
            icon, stage.id, status_text, duration
        );
    }
    println!();

    // Flush stdout to ensure output is visible
    let _ = io::stdout().flush();
}

/// Print a simple ASCII execution graph showing stage dependencies.
fn print_execution_graph(stages: &[StageCompletionInfo]) {
    // Compute dependency levels using shared function
    let levels = levels::compute_all_levels(stages, |s| s.id.as_str(), |s| &s.dependencies);

    // Group stages by level
    let mut stages_by_level: HashMap<usize, Vec<&StageCompletionInfo>> = HashMap::new();
    for stage in stages {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        stages_by_level.entry(level).or_default().push(stage);
    }

    let max_level = levels.values().copied().max().unwrap_or(0);

    // Print stages level by level
    for level in 0..=max_level {
        if let Some(level_stages) = stages_by_level.get(&level) {
            let stage_names: Vec<String> = level_stages
                .iter()
                .map(|s| {
                    let short_id = if s.id.len() > 20 {
                        format!("{}...", &s.id[..17])
                    } else {
                        s.id.clone()
                    };
                    format!("[{short_id}]")
                })
                .collect();

            let indent = "  ".repeat(level);

            if level == 0 {
                println!("  {}", stage_names.join(" "));
            } else if level_stages.len() == 1 {
                println!("  {indent}│");
                println!("  {indent}▼");
                println!("  {}", stage_names[0]);
            } else {
                println!("  {indent}│");
                println!("  {indent}├─► {}", stage_names.join(" "));
            }
        }
    }
}
