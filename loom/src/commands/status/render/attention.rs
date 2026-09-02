//! Attention/failure details widget (verbose mode)

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StageSummary;
use crate::models::stage::StageStatus;

/// Render detailed failure information for blocked stages. The status line,
/// ID, `Reason:`, and `Hint:` always render for a problem stage; `verbose`
/// only gates the `Evidence:` listing (see `render_failure_evidence`).
pub fn render_attention<W: Write>(
    w: &mut W,
    stages: &[StageSummary],
    verbose: bool,
) -> std::io::Result<()> {
    let problem_stages: Vec<_> = stages
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                StageStatus::Blocked
                    | StageStatus::MergeConflict
                    | StageStatus::CompletedWithFailures
                    | StageStatus::MergeBlocked
                    | StageStatus::NeedsHumanReview
            ) || s.cleanup_warning.is_some()
        })
        .collect();

    if problem_stages.is_empty() {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "⚠ Requires Attention".red().bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    for stage in problem_stages {
        render_problem_stage(w, stage, verbose)?;
        // The other problem statuses have one obvious next command; a stage
        // stopped for a human has three, so it gets the extra lines spelling
        // them out instead of leaving the operator to `human-review` with no
        // flags to see them.
        if stage.status == StageStatus::NeedsHumanReview {
            render_human_review_choices(w)?;
        }
    }

    Ok(())
}

/// The three `loom stage human-review` actions, indented under the hint line
/// `render_problem_stage` already printed for a `NeedsHumanReview` stage.
/// Kept out of `render_problem_stage` itself so that the pinned
/// function's line count does not grow.
fn render_human_review_choices<W: Write>(w: &mut W) -> std::io::Result<()> {
    const CHOICES: [(&str, &str); 3] = [
        ("--approve", "queue a fresh session with fresh fix attempts"),
        ("--force-complete", "skip acceptance and mark completed"),
        ("--reject <reason>", "block the stage"),
    ];
    for (flag, description) in CHOICES {
        writeln!(
            w,
            "          {}",
            format!("{flag:<19}{description}").dimmed()
        )?;
    }
    Ok(())
}

fn render_problem_stage<W: Write>(
    w: &mut W,
    stage: &StageSummary,
    verbose: bool,
) -> std::io::Result<()> {
    // A cleanup warning can land on any stage status (orphan cleanup also runs
    // on Skipped stages, not just Completed), so it is decided before status.
    if stage.cleanup_warning.is_some() {
        return render_cleanup_warning(w, stage);
    }
    let status_str = match &stage.status {
        StageStatus::Blocked => "BLOCKED",
        StageStatus::MergeConflict => "MERGE CONFLICT",
        StageStatus::CompletedWithFailures => "ACCEPTANCE FAILED",
        StageStatus::MergeBlocked => "MERGE ERROR",
        StageStatus::NeedsHumanReview => "NEEDS REVIEW",
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

    // Show failure type unconditionally; the evidence listing is gated
    // behind --verbose (see render_failure_evidence).
    if let Some(ref failure) = stage.failure_info {
        writeln!(w, "    Type: {:?}", failure.failure_type)?;
    }
    render_failure_evidence(w, stage, verbose)?;

    // Show review reason if available (NeedsHumanReview stages)
    if let Some(ref reason) = stage.review_reason {
        writeln!(w, "    Reason: {}", reason.yellow())?;
    }

    // Suggest recovery action
    let hint = match &stage.status {
        StageStatus::Blocked => format!("loom stage retry {}", stage.id),
        StageStatus::MergeConflict => format!("loom stage merge {}", stage.id),
        StageStatus::CompletedWithFailures => format!("loom stage retry {}", stage.id),
        StageStatus::MergeBlocked => format!("loom stage merge {}", stage.id),
        StageStatus::NeedsHumanReview => format!("loom stage human-review {}", stage.id),
        _ => "loom status".to_string(),
    };
    writeln!(w, "    {}: {}", "Hint".cyan(), hint.dimmed())?;

    Ok(())
}

/// Prints the `Evidence:` listing from `stage.failure_info` (up to five
/// lines plus an "... N more lines" tail) only when `verbose` is set. Kept
/// out of `render_problem_stage` itself so that the pinned function's line
/// count does not grow.
fn render_failure_evidence<W: Write>(
    w: &mut W,
    stage: &StageSummary,
    verbose: bool,
) -> std::io::Result<()> {
    let Some(ref failure) = stage.failure_info else {
        return Ok(());
    };
    if !verbose || failure.evidence.is_empty() {
        return Ok(());
    }
    writeln!(w, "    Evidence:")?;
    for line in failure.evidence.iter().take(5) {
        writeln!(w, "      {}", line.dimmed())?;
    }
    if failure.evidence.len() > 5 {
        writeln!(w, "      ... {} more lines", failure.evidence.len() - 5)?;
    }
    Ok(())
}

/// Render a stage entirely on account of a failed/refused deferred cleanup:
/// header, ID, warning body, and its `loom worktree remove` hint. Takes over
/// the whole presentation regardless of `stage.status` — cleanup runs on
/// Skipped stages too, not just Completed. Only called when
/// `stage.cleanup_warning` is `Some`.
fn render_cleanup_warning<W: Write>(w: &mut W, stage: &StageSummary) -> std::io::Result<()> {
    writeln!(
        w,
        "\n  {} {} (CLEANUP FAILED)",
        "►".red(),
        stage.name.red().bold()
    )?;
    writeln!(w, "    ID: {}", stage.id.dimmed())?;
    if let Some(ref warning) = stage.cleanup_warning {
        writeln!(w, "    Cleanup warning:")?;
        for line in warning.lines() {
            writeln!(w, "      {}", line.yellow())?;
        }
    }
    let hint = format!("loom worktree remove {}", stage.id);
    writeln!(w, "    {}: {}", "Hint".cyan(), hint.dimmed())?;
    Ok(())
}

#[cfg(test)]
#[path = "attention_tests.rs"]
mod tests;
