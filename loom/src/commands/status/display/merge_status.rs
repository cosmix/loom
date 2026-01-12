use colored::Colorize;

use crate::commands::status::merge_status::MergeStatusReport;

/// Display the merge status report in a formatted way.
///
/// Shows merged stages, conflict stages, pending merges, and any warnings.
pub fn display_merge_status(report: &MergeStatusReport) {
    // Skip section entirely if nothing to show
    if report.total() == 0 && report.warnings.is_empty() {
        return;
    }

    println!("\n{}", "Merge Status".bold());

    if !report.merged.is_empty() {
        println!(
            "\n  {} {}",
            "✓".green(),
            format!("Already merged: {}", report.merged.join(", ")).green()
        );
    }

    if !report.conflicts.is_empty() {
        println!("\n  {} Conflicts ({}):", "!".red(), report.conflicts.len());
        for stage_id in &report.conflicts {
            println!("    {stage_id}  loom attach {stage_id}");
        }
    }

    if !report.pending.is_empty() {
        println!("\n  {} Pending ({}):", "○".yellow(), report.pending.len());
        for (i, stage_id) in report.pending.iter().enumerate() {
            println!("    {}. {}      loom merge {}", i + 1, stage_id, stage_id);
        }
    }

    if !report.warnings.is_empty() {
        println!("\n  {} Warnings:", "⚠".yellow());
        for warning in &report.warnings {
            println!("    - {warning}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_merge_status_empty() {
        let report = MergeStatusReport::default();
        // Just verify it doesn't panic
        display_merge_status(&report);
    }

    #[test]
    fn test_display_merge_status_with_merged() {
        let mut report = MergeStatusReport::default();
        report.merged.push("stage-1".to_string());
        report.merged.push("stage-2".to_string());
        // Just verify it doesn't panic
        display_merge_status(&report);
    }

    #[test]
    fn test_display_merge_status_with_conflicts() {
        let mut report = MergeStatusReport::default();
        report.conflicts.push("stage-conflict".to_string());
        // Just verify it doesn't panic
        display_merge_status(&report);
    }

    #[test]
    fn test_display_merge_status_with_pending() {
        let mut report = MergeStatusReport::default();
        report.pending.push("stage-pending-1".to_string());
        report.pending.push("stage-pending-2".to_string());
        // Just verify it doesn't panic
        display_merge_status(&report);
    }

    #[test]
    fn test_display_merge_status_with_warnings() {
        let mut report = MergeStatusReport::default();
        report
            .warnings
            .push("Stage 'foo' branch missing".to_string());
        // Just verify it doesn't panic
        display_merge_status(&report);
    }

    #[test]
    fn test_display_merge_status_mixed() {
        let mut report = MergeStatusReport::default();
        report.merged.push("stage-done".to_string());
        report.pending.push("stage-pending".to_string());
        report.conflicts.push("stage-conflict".to_string());
        report.warnings.push("Some warning".to_string());
        // Just verify it doesn't panic
        display_merge_status(&report);
    }
}
