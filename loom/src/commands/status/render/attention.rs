//! Attention/failure details widget (verbose mode)

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StageSummary;

use super::attention_model::{attention_entries, AttentionEntry};

/// Render detailed failure information for blocked stages. The status line,
/// ID, `Reason:`, and `Hint:` always render for a problem stage; `verbose`
/// only gates the `Evidence:` listing (see `render_failure_evidence`).
pub fn render_attention<W: Write>(
    w: &mut W,
    stages: &[StageSummary],
    verbose: bool,
) -> std::io::Result<()> {
    let problem_stages = attention_entries(stages);

    if problem_stages.is_empty() {
        return Ok(());
    }

    writeln!(w)?;
    writeln!(w, "{}", "⚠ Requires Attention".red().bold())?;
    writeln!(w, "{}", "─".repeat(50))?;

    for stage in &problem_stages {
        render_problem_stage(w, stage, verbose)?;
        // The other problem statuses have one obvious next command; a stage
        // stopped for a human has three, so it gets the extra lines spelling
        // them out instead of leaving the operator to `human-review` with no
        // flags to see them.
        if stage.has_human_review_choices {
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
    entry: &AttentionEntry,
    verbose: bool,
) -> std::io::Result<()> {
    if entry.cleanup_warning.is_some() {
        return render_cleanup_warning(w, entry);
    }

    writeln!(
        w,
        "\n  {} {} ({})",
        "►".red(),
        entry.name.red().bold(),
        entry.label
    )?;
    writeln!(w, "    ID: {}", entry.id.dimmed())?;

    // Show failure type unconditionally; the evidence listing is gated
    // behind --verbose (see render_failure_evidence).
    if let Some(ref failure_type) = entry.failure_type {
        writeln!(w, "    Type: {failure_type:?}")?;
    }
    render_failure_evidence(w, &entry.evidence, verbose)?;

    // Show review reason if available (NeedsHumanReview stages)
    if let Some(ref reason) = entry.review_reason {
        writeln!(w, "    Reason: {}", reason.yellow())?;
    }
    render_adjudication_reason(w, entry)?;
    writeln!(w, "    {}: {}", "Hint".cyan(), entry.hint.dimmed())?;

    Ok(())
}

/// Prints the `Evidence:` listing from `stage.failure_info` (up to five
/// lines plus an "... N more lines" tail) only when `verbose` is set. Kept
/// out of `render_problem_stage` itself so that the pinned function's line
/// count does not grow.
fn render_failure_evidence<W: Write>(
    w: &mut W,
    evidence: &[String],
    verbose: bool,
) -> std::io::Result<()> {
    if !verbose || evidence.is_empty() {
        return Ok(());
    }
    writeln!(w, "    Evidence:")?;
    for line in evidence.iter().take(5) {
        writeln!(w, "      {}", line.dimmed())?;
    }
    if evidence.len() > 5 {
        writeln!(w, "      ... {} more lines", evidence.len() - 5)?;
    }
    Ok(())
}

fn render_adjudication_reason<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    let Some(dispute_count) = entry.dispute_count else {
        return Ok(());
    };
    let reason = match entry.judge_heartbeat_secs {
        Some(heartbeat_secs) => {
            format!("{dispute_count} disputes filed; judge heartbeat {heartbeat_secs}s ago")
        }
        None => format!("{dispute_count} disputes filed"),
    };
    writeln!(w, "    Reason: {}", reason.yellow())
}

/// Render a stage entirely on account of a failed/refused deferred cleanup:
/// header, ID, warning body, and its `loom worktree remove` hint. Takes over
/// the whole presentation regardless of `stage.status` — cleanup runs on
/// Skipped stages too, not just Completed. Only called when
/// `stage.cleanup_warning` is `Some`.
fn render_cleanup_warning<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    writeln!(
        w,
        "\n  {} {} (CLEANUP FAILED)",
        "►".red(),
        entry.name.red().bold()
    )?;
    writeln!(w, "    ID: {}", entry.id.dimmed())?;
    if let Some(ref warning) = entry.cleanup_warning {
        writeln!(w, "    Cleanup warning:")?;
        for line in warning.lines() {
            writeln!(w, "      {}", line.yellow())?;
        }
    }
    writeln!(w, "    {}: {}", "Hint".cyan(), entry.hint.dimmed())?;
    Ok(())
}

#[cfg(test)]
#[path = "attention_tests.rs"]
mod tests;
