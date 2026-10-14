//! One needs-attention entry as the ledger lays it out: the header line, the
//! first line of evidence, the note wrapped under it, the command with when to
//! run it, the alternatives each under a line saying when it applies, and a
//! pointer to the acceptance criteria they number.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::text::{self, cut_line};
use crate::commands::status::render::attention_model::{
    failure_label, human_review_choices, Alternative, AttentionEntry,
};
use crate::commands::status::render::wrap::wrap_text;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::models::stage::StageStatus;

/// Indent of the lines under an entry's header line.
const DETAIL_INDENT: &str = "                       ";

/// Indent of a command line. A command's end is what tells two alternatives
/// apart (`--op delete` from `--op replace`), so it starts well left of the
/// detail lines and keeps the width for the words that differ.
const COMMAND_INDENT: &str = "  ";

/// Most lines of an entry's note the panel shows. The panel holds a handful of
/// rows, and the note comes before the command: a longer note would push the
/// command out of view. `loom status` prints the note whole.
const MAX_NOTE_LINES: usize = 3;

/// Where the numbered acceptance criteria are: the alternatives that amend one
/// take its `--index`, and the panel has no room to list them.
const ACCEPTANCE_POINTER: &str = "Acceptance criteria are numbered from 0 in `loom status`";

/// The lines of one entry, each at most `width` cells wide.
pub(super) fn entry_lines(entry: &AttentionEntry, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![header_line(entry, width)];
    if let Some(evidence) = entry.evidence.first() {
        lines.push(detail_line(evidence, width));
    }
    lines.extend(note_lines(entry, width));
    lines.extend(command_lines(entry, width));
    for alternative in &entry.alternatives {
        lines.extend(alternative_lines(alternative, width));
    }
    if !entry.acceptance.is_empty() {
        lines.push(detail_line(ACCEPTANCE_POINTER, width));
    }
    if entry.has_human_review_choices {
        lines.extend(
            human_review_choices(&entry.id)
                .into_iter()
                .map(|(command, _)| command_line(command, width)),
        );
    }
    lines
}

fn header_line(entry: &AttentionEntry, width: u16) -> Line<'static> {
    let status = entry_status(entry.label);
    let detail = attention_detail(entry);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(status.icon().to_owned(), status.tui_style()),
        Span::raw(format!(" {} ", text::padded(&entry.id, 20))),
        Span::styled(
            entry.label.to_owned(),
            status.tui_style().add_modifier(Modifier::BOLD),
        ),
    ];
    if !detail.is_empty() {
        spans.push(Span::raw(" · "));
        let used: usize = spans
            .iter()
            .map(|span| text::text_width(&span.content))
            .sum();
        let room = usize::from(width).saturating_sub(used);
        spans.push(Span::raw(text::truncate(&detail, room)));
    }
    cut_line(Line::from(spans), width)
}

/// `cleanup_warning` is already flattened to one line (and capped at
/// `MAX_INLINE_CHARS`) by `context::untrusted::inline_safe` before it reaches
/// a `StageSummary`, so it is used directly rather than split on newlines.
fn attention_detail(entry: &AttentionEntry) -> String {
    entry
        .review_reason
        .clone()
        .or_else(|| {
            entry
                .failure_type
                .as_ref()
                .map(failure_label)
                .map(str::to_owned)
        })
        .or_else(|| entry.cleanup_warning.clone())
        .unwrap_or_default()
}

/// `text` under the header, dimmed, ending in `…` where it is cut to `width`.
fn detail_line(text: &str, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        text::truncate(&format!("{DETAIL_INDENT}{text}"), usize::from(width)),
        Theme::dimmed(),
    ))
}

/// The entry's note wrapped to the room under the header, at most
/// [`MAX_NOTE_LINES`] lines, the last ending in `…` when the note goes on. A
/// note alone is prose, so it gets no command arrow.
fn note_lines(entry: &AttentionEntry, width: u16) -> Vec<Line<'static>> {
    let Some(note) = entry.note.as_deref() else {
        return Vec::new();
    };
    let room = usize::from(width).saturating_sub(DETAIL_INDENT.len());
    let mut wrapped: Vec<String> = wrap_text(note, room)
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect();
    if wrapped.len() > MAX_NOTE_LINES {
        wrapped.truncate(MAX_NOTE_LINES);
        if let Some(last) = wrapped.last_mut() {
            *last = text::truncate(&format!("{last}…"), room);
        }
    }
    wrapped
        .iter()
        .map(|line| detail_line(line, width))
        .collect()
}

/// The entry's command, preceded by when to run it when that is a later step.
fn command_lines(entry: &AttentionEntry, width: u16) -> Vec<Line<'static>> {
    let Some(command) = entry.command.clone() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(when) = entry.command_when.as_deref() {
        lines.push(detail_line(&format!("Once {when}:"), width));
    }
    lines.push(command_line(command, width));
    lines
}

/// An alternative as two lines, when it applies and then its command. The
/// situations are long and the commands differ near their end, so on one line
/// the cut at the panel's width would hide what tells two alternatives apart.
fn alternative_lines(alternative: &Alternative, width: u16) -> [Line<'static>; 2] {
    [
        detail_line(&format!("If {}:", alternative.when), width),
        command_line(alternative.command.clone(), width),
    ]
}

/// `→ command` under the entry's detail, ending in `…` where it is cut to
/// `width`.
fn command_line(command: String, width: u16) -> Line<'static> {
    let arrow = format!("{COMMAND_INDENT}→ ");
    let room = usize::from(width).saturating_sub(text::text_width(&arrow));
    cut_line(
        Line::from(vec![
            Span::styled(arrow, Theme::dimmed()),
            Span::styled(
                text::truncate(&command, room),
                Style::default().fg(StatusColors::QUEUED),
            ),
        ]),
        width,
    )
}

fn entry_status(label: &str) -> StageStatus {
    match label {
        "MERGE CONFLICT" => StageStatus::MergeConflict,
        "ACCEPTANCE FAILED" => StageStatus::CompletedWithFailures,
        "MERGE ERROR" | "MERGE BLOCKED" => StageStatus::MergeBlocked,
        // Changes at risk read as the strongest state.
        "STASH NOT RESTORED" => StageStatus::Blocked,
        "NEEDS REVIEW" => StageStatus::NeedsHumanReview,
        "NEEDS INPUT" => StageStatus::WaitingForInput,
        "ADJUDICATING" => StageStatus::NeedsAdjudication,
        _ => StageStatus::Blocked,
    }
}
