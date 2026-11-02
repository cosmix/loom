//! Shared completion summary rendering for both TUI and foreground modes.

use std::collections::HashMap;
use std::io::{self, Write};

use crate::daemon::{CompletionSummary, StageCompletionInfo};
use crate::plan::graph::levels;

use crate::utils::format_elapsed_verbose;

/// Print completion summary to stdout.
///
/// Used by both the TUI (after exit) and foreground mode.
pub fn print_completion_summary(summary: &CompletionSummary) {
    let mut stdout = io::stdout().lock();
    // A closed stdout must not turn a finished run into a failure.
    let _ = write_completion_summary(&mut stdout, summary).and_then(|()| stdout.flush());
}

/// Write the overall status, plan path, timing, execution graph and stage table.
fn write_completion_summary<W: Write>(w: &mut W, summary: &CompletionSummary) -> io::Result<()> {
    let success = summary.failure_count == 0;
    let status_icon = if success { "\u{2713}" } else { "\u{2717}" };
    let status_text = if success {
        "Orchestration Complete"
    } else {
        "Orchestration Complete (with failures)"
    };

    writeln!(w)?;
    writeln!(w, "{status_icon} {status_text}")?;
    writeln!(w, "══════════════════════════════════════════════════")?;
    writeln!(w)?;

    writeln!(w, "Plan: {}", summary.plan_path)?;

    let total_time = format_elapsed_verbose(summary.total_duration_secs);
    let success_count = summary.success_count;
    let failure = summary.failure_count;
    writeln!(
        w,
        "Total: {total_time} | \u{2713} {success_count} | \u{2717} {failure}"
    )?;
    writeln!(w)?;

    if !summary.stages.is_empty() {
        writeln!(w, "Execution Graph:")?;
        write_execution_graph(w, &summary.stages)?;
        writeln!(w)?;
    }

    writeln!(w, "Stages:")?;
    for stage in &summary.stages {
        let icon = stage.status.icon();
        let status_text = stage.status.label();
        let duration = stage
            .duration_secs
            .map(format_elapsed_verbose)
            .unwrap_or_else(|| "-".to_string());
        writeln!(
            w,
            "  {} {:<30} {:<12} {:>8}",
            icon, stage.id, status_text, duration
        )?;
    }
    writeln!(w)
}

/// Write a simple ASCII execution graph showing stage dependencies.
fn write_execution_graph<W: Write>(w: &mut W, stages: &[StageCompletionInfo]) -> io::Result<()> {
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
                writeln!(w, "  {}", stage_names.join(" "))?;
            } else if level_stages.len() == 1 {
                writeln!(w, "  {indent}│")?;
                writeln!(w, "  {indent}▼")?;
                writeln!(w, "  {}", stage_names[0])?;
            } else {
                writeln!(w, "  {indent}│")?;
                writeln!(w, "  {indent}├─► {}", stage_names.join(" "))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;

    fn stage(id: &str, status: StageStatus, dependencies: &[&str]) -> StageCompletionInfo {
        StageCompletionInfo {
            id: id.to_string(),
            name: id.to_string(),
            status,
            duration_secs: Some(120),
            execution_secs: None,
            retry_count: 0,
            merged: false,
            dependencies: dependencies.iter().map(ToString::to_string).collect(),
        }
    }

    fn rendered(summary: &CompletionSummary) -> String {
        let mut out = Vec::new();
        write_completion_summary(&mut out, summary).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_successful_run_shows_its_plan_counts_graph_and_stages() {
        let text = rendered(&CompletionSummary {
            stages: vec![
                stage("bootstrap", StageStatus::Completed, &[]),
                stage("implement", StageStatus::Completed, &["bootstrap"]),
            ],
            total_duration_secs: 180,
            success_count: 2,
            failure_count: 0,
            plan_path: "doc/plans/PLAN-test.md".to_string(),
        });
        assert!(text.contains("\u{2713} Orchestration Complete\n"), "{text}");
        assert!(text.contains("Plan: doc/plans/PLAN-test.md"), "{text}");
        assert!(text.contains("\u{2713} 2 | \u{2717} 0"), "{text}");
        assert!(text.contains("[bootstrap]"), "{text}");
        assert!(text.contains("[implement]"), "{text}");
    }

    #[test]
    fn any_failure_changes_the_header_and_count() {
        let text = rendered(&CompletionSummary {
            stages: vec![stage("failing", StageStatus::Blocked, &[])],
            total_duration_secs: 60,
            success_count: 0,
            failure_count: 1,
            plan_path: "unknown".to_string(),
        });
        assert!(
            text.contains("\u{2717} Orchestration Complete (with failures)\n"),
            "{text}"
        );
        assert!(text.contains("\u{2713} 0 | \u{2717} 1"), "{text}");
        assert!(text.contains("failing"), "{text}");
    }
}
