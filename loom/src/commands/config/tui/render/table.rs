//! The key table: every registry key, grouped by section, both tiers visible.
//!
//! The row the operator is on is drawn for the ACTIVE scope; the other tier
//! rides along in the OTHER column so switching tabs is never the only way to
//! find out what the other file sets.
//!
//! A row's KEY cell carries the bare field name. The section heading above it
//! already says `models`, and repeating it on every row costs seven to nine
//! columns that a narrow terminal needs for values.

use ratatui::{
    layout::{Constraint, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Cell, Row, Table, TableState},
    Frame,
};

use super::super::state::{ConfigRow, ConfigState, Pending, Scope, Source};
use super::super::theme;
use crate::commands::status::ui::tui::ledger::text::{text_width, truncate};
use crate::user_config::{
    keys::{ValueKind, KEYS},
    ConfigValue,
};

/// The KEY column's width: exactly the longest field name in the registry, so
/// no key is ever truncated and no column is wider than it needs to be.
///
/// Derived rather than written down. A hand-counted width is wrong the first
/// time a longer key is added, and silently — the key just loses its tail.
const KEY_WIDTH: u16 = widest_field();

/// The longest `KeySpec::field` in [`KEYS`], in bytes.
///
/// `len()` rather than a display-width measurement because this runs in a
/// const context, where nothing unicode-aware can be called. That is sound
/// only because every field name is an ASCII TOML identifier, which is not a
/// property a reader should have to take on trust — `tests::the_key_column_fits_every_registry_field`
/// pins it against the real measurement.
const fn widest_field() -> u16 {
    let mut widest = 0;
    let mut index = 0;
    while index < KEYS.len() {
        let width = KEYS[index].field.len();
        if width > widest {
            widest = width;
        }
        index += 1;
    }
    widest as u16
}

/// Inner width below which OTHER goes: it repeats a fact the inspector states
/// in full, so it is the first thing a cramped terminal gives up.
const DROP_OTHER: usize = 64;
/// Inner width below which SOURCE goes too. KEY and VALUE never go.
const DROP_SOURCE: usize = 52;
/// The minimums VALUE and OTHER declare, which is the ratio the layout solver
/// hands out slack in.
const VALUE_MIN: usize = 16;
/// See [`VALUE_MIN`].
const OTHER_MIN: usize = 14;

/// How much room the flexible columns actually got, so their text can be
/// truncated with an ellipsis rather than hard-clipped at the cell edge.
struct Flex {
    /// Cells available to the OTHER column, zero when it is not rendered.
    other: usize,
    /// How many of the five columns this layout renders.
    columns: usize,
}

/// Render the key table, scrolled so the selection stays visible.
pub(super) fn render(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let (constraints, flex) = layout(area.width);
    let mut header = vec!["", "KEY", "VALUE", "SOURCE", "OTHER"];
    header.truncate(flex.columns);
    let table = Table::new(rows(state, &flex), constraints)
        .header(
            Row::new(header)
                .style(theme::secondary().add_modifier(Modifier::BOLD))
                .bottom_margin(1),
        )
        .row_highlight_style(theme::selected())
        .block(theme::block(theme::chrome_title("keys")));
    let mut table_state = TableState::new().with_selected(Some(display_index(state)));
    frame.render_stateful_widget(table, area, &mut table_state);
}

/// The columns this width can carry, and how much room the flexible ones got.
///
/// The flexible widths are computed rather than read back from the layout
/// solver: they are only used to place an ellipsis, and being a cell or two
/// out costs a slightly early `…`, never a misdrawn row.
fn layout(width: u16) -> (Vec<Constraint>, Flex) {
    let inner = usize::from(width.saturating_sub(2));
    let marker = Constraint::Length(2);
    let key = Constraint::Length(KEY_WIDTH);
    let value = Constraint::Min(VALUE_MIN as u16);
    let source = Constraint::Length(11);
    if inner < DROP_SOURCE {
        return (
            vec![marker, key, value],
            Flex {
                other: 0,
                columns: 3,
            },
        );
    }
    if inner < DROP_OTHER {
        return (
            vec![marker, key, value, source],
            Flex {
                other: 0,
                columns: 4,
            },
        );
    }
    // VALUE and OTHER share the slack in the ratio of their own minimums, the
    // way the solver hands out the excess. Four gaps sit between five columns.
    let slack = inner.saturating_sub(2 + usize::from(KEY_WIDTH) + 11 + 4);
    (
        vec![
            marker,
            key,
            value,
            source,
            Constraint::Min(OTHER_MIN as u16),
        ],
        Flex {
            other: slack * OTHER_MIN / (VALUE_MIN + OTHER_MIN),
            columns: 5,
        },
    )
}

/// Every registry row in `KEYS` order, with a heading row opening each section.
fn rows(state: &ConfigState, flex: &Flex) -> Vec<Row<'static>> {
    let mut rendered = Vec::new();
    let mut section = None;
    for (index, row) in state.rows().iter().enumerate() {
        if section != Some(row.spec().section) {
            section = Some(row.spec().section);
            rendered.push(heading(row.spec().section, flex));
        }
        rendered.push(key_row(row, index == state.selected(), state, flex));
    }
    rendered
}

/// A section heading, ruled out to the KEY column's width. Never selectable:
/// selection indices only ever address config rows.
fn heading(section: &str, flex: &Flex) -> Row<'static> {
    let label = format!("── {section} ");
    let rule = "─".repeat(usize::from(KEY_WIDTH).saturating_sub(text_width(&label)));
    let mut cells = vec![
        Cell::from(""),
        Cell::from(Line::from(Span::styled(
            format!("{label}{rule}"),
            theme::rule(),
        ))),
    ];
    cells.resize_with(flex.columns, || Cell::from(""));
    Row::new(cells)
}

/// One registry key, rendered for the active scope.
fn key_row(row: &ConfigRow, selected: bool, state: &ConfigState, flex: &Flex) -> Row<'static> {
    let scope = state.scope();
    let mut cells = vec![
        marker_cell(selected, row.is_modified(scope)),
        key_cell(row, scope),
        value_cell(row, selected, state),
        source_cell(row, scope),
        other_cell(row, scope, flex.other),
    ];
    cells.truncate(flex.columns);
    Row::new(cells)
}

/// Two cells: the selection caret, then the staged-edit dot.
fn marker_cell(selected: bool, pending: bool) -> Cell<'static> {
    Cell::from(Line::from(vec![
        Span::styled(if selected { "▸" } else { " " }, theme::staged()),
        Span::styled(if pending { "•" } else { " " }, theme::staged()),
    ]))
}

/// The bare field name; the section heading above the row supplies the rest.
/// The cell dims on a row the active scope cannot touch — an unavailable row
/// should read as unavailable before the operator looks at its value.
///
/// Derived from whether the scope resolves a value at all, rather than from
/// matching the specific `ProjectTier` variant that means "no project tier
/// exists": `disk_value` already returns `None` for every reason a scope can
/// be untouchable, so a third one added later dims here without an edit.
fn key_cell(row: &ConfigRow, scope: Scope) -> Cell<'static> {
    let style = if row.disk_value(scope).is_none() {
        theme::secondary()
    } else {
        theme::primary()
    };
    Cell::from(Line::from(Span::styled(row.spec().field, style)))
}

/// The active scope's value, by the key's registry kind.
fn value_cell(row: &ConfigRow, selected: bool, state: &ConfigState) -> Cell<'static> {
    let scope = state.scope();
    if selected && state.is_editing() {
        let buffer = state.edit_buffer().unwrap_or_default();
        return Cell::from(Line::from(Span::styled(
            format!("{buffer}▏"),
            theme::staged().bg(theme::SELECT_BG),
        )));
    }
    let Some(value) = row.displayed(scope) else {
        return Cell::from(Line::from(Span::styled("—", theme::secondary())));
    };
    let cycles = selected && matches!(row.spec().kind, ValueKind::Enum(_));
    let text = match (&row.spec().kind, &value) {
        (ValueKind::Bool, ConfigValue::Bool(true)) => "◉ on".to_owned(),
        (ValueKind::Bool, ConfigValue::Bool(false)) => "◯ off".to_owned(),
        // The guillemets are the affordance saying ←/→ steps this value.
        _ if cycles => format!("‹ {value} ›"),
        _ => row
            .displayed_text(scope)
            .unwrap_or_else(|| value.to_string()),
    };
    let style = if row.is_modified(scope) || cycles {
        theme::staged()
    } else {
        theme::primary()
    };
    let mut spans = vec![Span::styled(text, style)];
    if matches!(row.pending(scope), Some(Pending::Clear)) {
        spans.push(Span::styled(" (cleared)", theme::secondary()));
    }
    Cell::from(Line::from(spans))
}

/// Where the active scope's value comes from, and whether this tier is the one
/// the daemon actually reads.
fn source_cell(row: &ConfigRow, scope: Scope) -> Cell<'static> {
    let source = row.source(scope);
    let style = if source == Source::Set {
        theme::written()
    } else {
        theme::secondary()
    };
    Cell::from(Line::from(vec![
        Span::styled(
            if row.in_force().is(scope) {
                "◆ "
            } else {
                "  "
            },
            theme::written(),
        ),
        Span::styled(source.label(), style),
    ]))
}

/// What the other tier SETS, as it stands on disk.
///
/// `—` unless that tier's own file supplies the key. What the other tier
/// *resolves* to would be the wrong question: a project-backed key the project
/// file omits falls through to the user's value, so that reading would print
/// `project <the same value>` on nearly every row and tell the operator
/// nothing. A staged edit is left out for the same reason — it belongs to the
/// tab it was made on, and would read here as already written.
fn other_cell(row: &ConfigRow, scope: Scope, width: usize) -> Cell<'static> {
    let other = scope.other();
    let Some(value) = row.set_value(other) else {
        return Cell::from(Line::from(Span::styled("—", theme::secondary())));
    };
    let word = other.word();
    let room = width.saturating_sub(text_width(word) + 1);
    Cell::from(Line::from(vec![
        Span::styled(word, theme::plain(theme::scope_color(other))),
        Span::styled(
            format!(" {}", truncate(&value.to_string(), room)),
            theme::secondary(),
        ),
    ]))
}

/// The selected row's index among the DISPLAY rows, section headings included,
/// because the table widget scrolls by display row rather than by key.
fn display_index(state: &ConfigState) -> usize {
    let mut index = 0;
    let mut section = None;
    for (position, row) in state.rows().iter().enumerate() {
        if section != Some(row.spec().section) {
            section = Some(row.spec().section);
            index += 1;
        }
        if position == state.selected() {
            return index;
        }
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{key_cell, theme, Cell, ConfigRow, Line, Scope, Span, KEYS, KEY_WIDTH};
    use crate::commands::status::ui::tui::ledger::text::text_width;
    use crate::user_config::{workspace, UserConfig};

    /// The KEY column is derived with `len()` in a const context, which is a
    /// byte count. This is the check that the registry's field names really
    /// are ASCII, so that count really is a display width — and that the
    /// column really does fit the longest of them.
    #[test]
    fn the_key_column_fits_every_registry_field() {
        let widest = KEYS
            .iter()
            .map(|spec| text_width(spec.field))
            .max()
            .expect("the registry is never empty");
        assert_eq!(usize::from(KEY_WIDTH), widest);
    }

    /// The key cell dims at Project scope for BOTH reasons the row is
    /// untouchable there — no project tier exists for the key at all, or the
    /// tree has no workspace to hold one. Matching only the first once left
    /// every project-backed key at full brightness in a tree with no
    /// workspace, while every edit path refused it just the same.
    #[test]
    fn the_key_cell_dims_for_every_reason_project_scope_is_untouchable() {
        let unbacked = KEYS
            .iter()
            .find(|spec| !workspace::backs(spec))
            .expect("the registry has a user-only key");
        let backed = KEYS
            .iter()
            .find(|spec| workspace::backs(spec))
            .expect("the registry has a project-backed key");
        let config = UserConfig::default();

        // `workspace: None` is enough to reach both variants: `Unbacked`
        // follows from the key itself, `NoWorkspace` from there being no
        // workspace to open — neither reads a real tree.
        let unbacked_row = ConfigRow::new(unbacked, &config, None).expect("build row");
        let no_workspace_row = ConfigRow::new(backed, &config, None).expect("build row");

        assert_eq!(
            key_cell(&unbacked_row, Scope::Project),
            Cell::from(Line::from(Span::styled(unbacked.field, theme::secondary())))
        );
        assert_eq!(
            key_cell(&no_workspace_row, Scope::Project),
            Cell::from(Line::from(Span::styled(backed.field, theme::secondary())))
        );
        // The same row reads at full brightness on the tab it CAN be edited from.
        assert_eq!(
            key_cell(&unbacked_row, Scope::User),
            Cell::from(Line::from(Span::styled(unbacked.field, theme::primary())))
        );
    }
}
