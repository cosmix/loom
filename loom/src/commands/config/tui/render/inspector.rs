//! The selected key's detail: its help, its type, and all three tiers at once.
//!
//! The three tiers are the point of it. They say which file sets the key,
//! which tier the daemon actually reads, and what loom would fall back to —
//! the questions an operator otherwise answers by opening two files.
//!
//! Two forms, never none. [`render_panel`] is the tall column beside the
//! table; [`render_strip`] condenses the same facts onto two lines under it
//! when the terminal is too narrow for a second column. A width that made this
//! information vanish would leave the help text and the built-in default with
//! nowhere on the screen to live.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Wrap},
    Frame,
};

use super::super::state::{ConfigRow, ConfigState, Scope, Source, SCOPES};
use super::super::theme;
use crate::commands::status::ui::tui::ledger::text::{cut_line, padded, spans_width, text_width};
use crate::user_config::keys::ValueKind;

/// The tier column's width, which the three tier lines align on.
const TIER_WIDTH: u16 = 10;
/// The value column's width on a tier line.
const VALUE_WIDTH: u16 = 11;
/// The source column's width, so the `◆` lands in the same place every row.
const SOURCE_WIDTH: u16 = 11;
/// The tier block: one blank line and the three tiers.
const TIER_HEIGHT: u16 = 4;
/// The closing block: one blank line and the line naming what `s` writes.
const WRITE_HEIGHT: u16 = 2;

/// Render the tall panel beside the table.
pub(super) fn render_panel(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let row = state.selected_row();
    let block = key_block(row);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Three stacked areas rather than one paragraph: the summary has to wrap,
    // and `Wrap { trim: true }` strips leading whitespace, which would pull
    // the tier lines out of their columns. The tier block is anchored from the
    // bottom because ratatui gives no public way to ask how tall a wrap came
    // out, and a tier block that moves with the length of the help text is
    // harder to read than one that never moves at all.
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(TIER_HEIGHT),
            Constraint::Length(WRITE_HEIGHT),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(summary_lines(row)).wrap(Wrap { trim: true }),
        areas[0],
    );
    frame.render_widget(Paragraph::new(tier_lines(row, state.scope())), areas[1]);
    frame.render_widget(
        Paragraph::new(write_line(state)).wrap(Wrap { trim: true }),
        areas[2],
    );
}

/// Render the condensed strip under the table, for a frame too narrow to
/// carry a second column.
pub(super) fn render_strip(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let row = state.selected_row();
    let block = key_block(row);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(vec![
            strip_help(row, inner.width),
            strip_facts(row, state, inner.width),
        ]),
        inner,
    );
}

/// The bordered block both forms sit in, titled with the key's full dotted
/// name — the one place on the screen it still appears now that the table
/// shows bare field names.
///
/// Padded a column in, so its text does not sit against the border the way no
/// other block's does.
fn key_block(row: &ConfigRow) -> Block<'static> {
    theme::block(Line::from(Span::styled(
        format!(" {} ", row.spec().name),
        theme::emphasis(theme::GOLD),
    )))
    .padding(Padding::horizontal(1))
}

/// The strip's first line: the key's description, with an enum's variants
/// appended when both fit.
fn strip_help(row: &ConfigRow, width: u16) -> Line<'static> {
    let mut spans = vec![Span::styled(row.spec().help, theme::primary())];
    if let ValueKind::Enum(variants) = &row.spec().kind {
        let variants = format!("  {}", variants.join(" · "));
        if spans_width(&spans) + text_width(&variants) <= usize::from(width) {
            spans.push(Span::styled(variants, theme::secondary()));
        }
    }
    cut_line(Line::from(spans), width)
}

/// The strip's second line: the kind, all three tiers, the built-in default,
/// and the file `s` writes, separated by rules — the same facts the panel
/// stacks, laid flat.
///
/// The write segment is laid out FIRST and its width reserved out of the
/// budget, because it is the one segment that must never drop: dropping it
/// would leave `s` a guess again. Everything else is cut to fit whatever room
/// is left, from the right — built-in first, then the tiers — through the
/// same `cut_line` every other narrow-width degradation on this screen goes
/// through. A final pass over the assembled line is only a safety net: the
/// two pieces already sum to at most `width`.
fn strip_facts(row: &ConfigRow, state: &ConfigState, width: u16) -> Line<'static> {
    let write = strip_write(state, usize::from(width));
    let room = (usize::from(width).saturating_sub(spans_width(&write))) as u16;

    let mut spans = vec![Span::styled(
        kind_word(&row.spec().kind),
        theme::secondary(),
    )];
    for scope in SCOPES {
        spans.push(separator());
        spans.extend(strip_tier(row, scope));
    }
    spans.push(separator());
    spans.push(Span::styled("built-in ", theme::secondary()));
    spans.push(Span::styled(row.built_in().to_string(), theme::primary()));

    let mut spans = cut_line(Line::from(spans), room).spans;
    spans.extend(write);
    cut_line(Line::from(spans), width)
}

/// One tier on the strip: `{tier} {value} {source}`, with `◆` when this is the
/// tier in force, or `{tier} —` when the scope has no value for the key.
fn strip_tier(row: &ConfigRow, scope: Scope) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!("{} ", scope.word()),
        theme::plain(theme::scope_color(scope)),
    )];
    let Some(value) = row.displayed(scope) else {
        spans.push(Span::styled("—", theme::secondary()));
        return spans;
    };
    let source = row.source(scope);
    spans.push(Span::styled(format!("{value} "), theme::primary()));
    spans.push(Span::styled(
        source.label(),
        if source == Source::Set {
            theme::written()
        } else {
            theme::secondary()
        },
    ));
    if row.in_force().is(scope) {
        spans.push(Span::styled(" ◆", theme::written()));
    }
    spans
}

/// The strip's closing segment: `s → <file>`, the path cut from the left so
/// its tail still names the file.
fn strip_write(state: &ConfigState, room: usize) -> Vec<Span<'static>> {
    let Some(path) = state.active_path() else {
        return vec![Span::styled("s writes nothing here", theme::secondary())];
    };
    let lead = "s → ";
    vec![
        Span::styled("s", theme::key_hint()),
        Span::styled(" → ", theme::secondary()),
        Span::styled(
            super::tail(path, room.saturating_sub(text_width(lead))),
            theme::primary(),
        ),
    ]
}

/// The rule that separates the strip's segments.
fn separator() -> Span<'static> {
    Span::styled(" · ", theme::rule())
}

/// The key's own description, then its type.
fn summary_lines(row: &ConfigRow) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(row.spec().help, theme::primary())),
        Line::default(),
        meta_line("kind", kind_word(&row.spec().kind).to_owned()),
    ];
    if let ValueKind::Enum(variants) = &row.spec().kind {
        lines.push(meta_line("variants", variants.join(" · ")));
    }
    lines
}

/// One `label  value` line of the type block.
fn meta_line(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(padded(label, TIER_WIDTH), theme::secondary()),
        Span::styled(value, theme::primary()),
    ])
}

/// The registry kind, in the words the operator sees elsewhere in loom.
fn kind_word(kind: &ValueKind) -> &'static str {
    match kind {
        ValueKind::Bool => "bool",
        ValueKind::Number => "number",
        ValueKind::String => "text",
        ValueKind::Enum(_) => "enum",
    }
}

/// The three tiers: both files and the built-in under them.
fn tier_lines(row: &ConfigRow, active: Scope) -> Vec<Line<'static>> {
    let mut lines = vec![Line::default()];
    for scope in SCOPES {
        lines.push(tier_line(row, scope, active));
    }
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(padded("built-in", TIER_WIDTH), theme::secondary()),
        Span::styled(
            padded(&row.built_in().to_string(), VALUE_WIDTH),
            theme::primary(),
        ),
    ]));
    lines
}

/// One tier: the scope the keyboard is on is marked, the tier in force is
/// marked separately — they are different questions and often different rows.
fn tier_line(row: &ConfigRow, scope: Scope, active: Scope) -> Line<'static> {
    let value = row
        .displayed(scope)
        .map_or_else(|| "—".to_owned(), |value| value.to_string());
    Line::from(vec![
        Span::styled(if scope == active { "▸ " } else { "  " }, theme::staged()),
        Span::styled(
            padded(scope.word(), TIER_WIDTH),
            theme::emphasis(theme::scope_color(scope)),
        ),
        Span::styled(padded(&value, VALUE_WIDTH), theme::primary()),
        Span::styled(
            padded(row.source(scope).label(), SOURCE_WIDTH),
            theme::secondary(),
        ),
        Span::styled(
            if row.in_force().is(scope) { "◆" } else { " " },
            theme::written(),
        ),
    ])
}

/// What `s` will do, named by file, so pressing it is never a guess.
fn write_line(state: &ConfigState) -> Vec<Line<'static>> {
    let spans = match state.active_path() {
        Some(path) => vec![
            Span::styled("s writes to ", theme::secondary()),
            Span::styled(path.to_owned(), theme::primary()),
        ],
        None => vec![Span::styled(
            "s writes nothing here — run loom init to create a project config",
            theme::secondary(),
        )],
    };
    vec![Line::default(), Line::from(spans)]
}
