//! Attention/failure details widget (verbose mode)

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StageSummary;
use crate::models::stage::StageStatus;

/// Render detailed failure information for blocked stages
pub fn render_attention<W: Write>(w: &mut W, stages: &[StageSummary]) -> std::io::Result<()> {
    let problem_stages: Vec<_> = stages
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                StageStatus::Blocked
                    | StageStatus::MergeConflict
                    | StageStatus::CompletedWithFailures
                    | StageStatus::MergeBlocked
            )
        })
        .collect();

    if problem_stages.is_empty() {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "⚠ Requires Attention".red().bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    for stage in problem_stages {
        render_problem_stage(w, stage)?;
    }

    Ok(())
}

fn render_problem_stage<W: Write>(w: &mut W, stage: &StageSummary) -> std::io::Result<()> {
    let status_str = match &stage.status {
        StageStatus::Blocked => "BLOCKED",
        StageStatus::MergeConflict => "MERGE CONFLICT",
        StageStatus::CompletedWithFailures => "ACCEPTANCE FAILED",
        StageStatus::MergeBlocked => "MERGE ERROR",
        _ => "ISSUE",
    };

    writeln!(
        w,
        "\n  {} {} ({})",
        "►".red(),
        stage.name.red().bold(),
        status_str
    )?;
    writeln!(w, "    ID: {}", stage.id.dimmed())?;

    // Show failure details if available
    if let Some(ref failure) = stage.failure_info {
        // Show failure type
        writeln!(w, "    Type: {:?}", failure.failure_type)?;

        if !failure.evidence.is_empty() {
            writeln!(w, "    Evidence:")?;
            for line in failure.evidence.iter().take(5) {
                writeln!(w, "      {}", line.dimmed())?;
            }
            if failure.evidence.len() > 5 {
                writeln!(w, "      ... {} more lines", failure.evidence.len() - 5)?;
            }
        }
    }

    // Suggest recovery action
    let hint = match &stage.status {
        StageStatus::Blocked => format!("loom stage retry {}", stage.id),
        StageStatus::MergeConflict => format!("loom merge {}", stage.id),
        StageStatus::CompletedWithFailures => format!("loom stage retry {}", stage.id),
        StageStatus::MergeBlocked => format!("loom merge {} --force", stage.id),
        _ => "loom status".to_string(),
    };
    writeln!(w, "    {}: {}", "Hint".cyan(), hint.dimmed())?;

    Ok(())
}
