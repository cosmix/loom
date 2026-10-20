//! Attention/failure details widget (verbose mode)

use colored::Colorize;
use std::io::Write;

use crate::commands::status::data::StageSummary;

use super::attention_model::{attention_entries, human_review_choices, AttentionEntry};
use super::wrap::wrap_text;

/// Render detailed failure information for blocked stages. The status line,
/// ID, `Reason:`, and the entry's guidance always render for a problem stage;
/// `verbose` only gates the `Evidence:` listing (see `render_failure_evidence`).
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
    }

    Ok(())
}

/// Columns assumed when the terminal's width cannot be read (output piped to a
/// file, for one).
const FALLBACK_WIDTH: usize = 100;

/// Narrowest text column a wrapped note gets, however narrow the terminal.
const MIN_TEXT_WIDTH: usize = 20;

/// Indent of every guidance line under an entry's header.
const INDENT: &str = "    ";

/// Indent of a command that sits on a line of its own under its introduction.
const COMMAND_INDENT: &str = "      ";

fn terminal_width() -> usize {
    crossterm::terminal::size().map_or(FALLBACK_WIDTH, |(columns, _)| usize::from(columns))
}

fn render_guidance<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    write_guidance(w, entry, terminal_width())
}

/// The entry's note, wrapped to `width` whole, then its command, then its
/// alternatives, and, for a stage stopped for a human, the three full review
/// commands. Prints nothing for a part the entry lacks.
///
/// A command that follows an introduction goes on a line of its own, unwrapped,
/// so it can be copied from the terminal as it is.
fn write_guidance<W: Write>(
    w: &mut W,
    entry: &AttentionEntry,
    width: usize,
) -> std::io::Result<()> {
    if let Some(note) = entry.text_note() {
        write_wrapped(w, "Note", &note, width)?;
    }
    match (entry.command.as_deref(), entry.command_when.as_deref()) {
        (Some(command), Some(when)) => write_step(w, &format!("Once {when}"), command, width)?,
        (Some(command), None) => writeln!(w, "{INDENT}{}: {}", "Run".cyan(), command.dimmed())?,
        (None, _) => {}
    }
    for alternative in &entry.alternatives {
        let when = format!("If {}", alternative.when);
        write_step(w, &when, &alternative.command, width)?;
    }
    write_acceptance(w, &entry.acceptance, width)?;
    if !entry.has_human_review_choices {
        return Ok(());
    }
    let choices = human_review_choices(&entry.id);
    let command_width = choices
        .iter()
        .map(|(command, _)| command.len())
        .max()
        .unwrap_or(0);
    writeln!(w, "{INDENT}{}:", "Run one of".cyan())?;
    for (command, description) in choices {
        writeln!(
            w,
            "{COMMAND_INDENT}{}  {description}",
            format!("{command:<command_width$}").dimmed()
        )?;
    }
    Ok(())
}

/// `label: body`, the body wrapped to `width` and its continuation lines
/// aligned under its first.
fn write_wrapped<W: Write>(
    w: &mut W,
    label: &str,
    body: &str,
    width: usize,
) -> std::io::Result<()> {
    let prefix_width = INDENT.len() + label.chars().count() + 2;
    let room = width.saturating_sub(prefix_width).max(MIN_TEXT_WIDTH);
    for (index, line) in wrap_text(body, room).iter().enumerate() {
        if index == 0 {
            writeln!(w, "{INDENT}{}: {}", label.cyan(), line.dimmed())?;
        } else {
            writeln!(w, "{}{}", " ".repeat(prefix_width), line.dimmed())?;
        }
    }
    Ok(())
}

/// The acceptance criteria an alternative above points to, each under the
/// number `loom stage amend --index` takes (the first is 0). A long command
/// wraps under its own first line. Prints nothing without criteria.
fn write_acceptance<W: Write>(w: &mut W, criteria: &[String], width: usize) -> std::io::Result<()> {
    if criteria.is_empty() {
        return Ok(());
    }
    writeln!(
        w,
        "{INDENT}{}",
        "Acceptance criteria (--index counts from 0):".cyan()
    )?;
    let digits = (criteria.len() - 1).to_string().len();
    let prefix_width = COMMAND_INDENT.len() + digits + 2;
    let room = width.saturating_sub(prefix_width).max(MIN_TEXT_WIDTH);
    for (index, criterion) in criteria.iter().enumerate() {
        let mut lines = wrap_text(criterion, room);
        if lines.is_empty() {
            lines.push(String::new());
        }
        for (row, line) in lines.iter().enumerate() {
            let number = if row == 0 {
                format!("{index:>digits$}")
            } else {
                " ".repeat(digits)
            };
            writeln!(w, "{COMMAND_INDENT}{}  {}", number.cyan(), line.dimmed())?;
        }
    }
    Ok(())
}

/// An introduction, wrapped, ending in a colon, with `command` on the line below.
fn write_step<W: Write>(
    w: &mut W,
    introduction: &str,
    command: &str,
    width: usize,
) -> std::io::Result<()> {
    let room = width.saturating_sub(INDENT.len()).max(MIN_TEXT_WIDTH);
    let lines = wrap_text(&format!("{introduction}:"), room);
    for line in &lines {
        writeln!(w, "{INDENT}{}", line.cyan())?;
    }
    writeln!(w, "{COMMAND_INDENT}{}", command.dimmed())
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

    if entry.completion_blocker.is_some() {
        render_completion_blocker(w, entry)?;
        return Ok(());
    }

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
    render_last_session(w, entry)?;
    render_guidance(w, entry)
}

fn render_completion_blocker<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    let Some(blocker) = entry.completion_blocker.as_ref() else {
        return Ok(());
    };
    writeln!(w, "    {}", blocker.activity_text().yellow())?;
    writeln!(w, "    fingerprint: {}", blocker.fingerprint.dimmed())?;
    writeln!(w, "    failure code: {}", blocker.failure_code.dimmed())?;
    writeln!(w, "    repeat count: {}", blocker.repeat_count)?;
    writeln!(w, "    commit: {}", blocker.commit.dimmed())?;
    if let Some(first) = blocker.first_observed_at.as_deref() {
        writeln!(w, "    first observed: {}", first.dimmed())?;
    }
    if let Some(last) = blocker.last_observed_at.as_deref() {
        writeln!(w, "    last observed: {}", last.dimmed())?;
    }
    render_last_session(w, entry)?;
    render_guidance(w, entry)
}

fn render_last_session<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    let Some(reason) = entry.outgoing_session_exit_reason else {
        return Ok(());
    };
    writeln!(w, "    last session: {}", reason.status_label())
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
/// header, ID, warning body, and its `loom worktree remove` command. Takes
/// over the whole presentation regardless of `stage.status` — cleanup runs on
/// Skipped stages too, not just Completed. Only called when
/// `stage.cleanup_warning` is `Some`.
///
/// `cleanup_warning` is flattened to one line and capped at
/// `MAX_INLINE_CHARS` by `context::untrusted::inline_safe` before it ever
/// reaches here (git's stderr is otherwise multi-line and can carry an ANSI
/// escape), so there is only ever one line to print — it points at the stage
/// file for the untruncated text.
fn render_cleanup_warning<W: Write>(w: &mut W, entry: &AttentionEntry) -> std::io::Result<()> {
    writeln!(
        w,
        "\n  {} {} (CLEANUP FAILED)",
        "►".red(),
        entry.name.red().bold()
    )?;
    writeln!(w, "    ID: {}", entry.id.dimmed())?;
    if let Some(ref warning) = entry.cleanup_warning {
        writeln!(
            w,
            "    Cleanup warning: {} (full text in the stage file)",
            warning.yellow()
        )?;
    }
    render_last_session(w, entry)?;
    render_guidance(w, entry)
}

#[cfg(test)]
#[path = "attention_tests.rs"]
mod tests;
