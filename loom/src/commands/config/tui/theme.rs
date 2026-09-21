//! The config editor's own palette.
//!
//! Deliberately NOT `StatusColors`: the status ledger's colours carry stage
//! meanings (executing, blocked, merged) that say nothing here, and a settings
//! screen borrowing them would drag the ledger's language along whenever
//! either screen changed. The two tier colours are the whole idea — a row's
//! tier is identifiable by colour alone, before the operator reads a word.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

use super::state::Scope;

/// The user tier, the logo, and structural accents.
pub(super) const WARP: Color = Color::Rgb(122, 162, 247);
/// The project tier — the other thread in the weave.
pub(super) const WEFT: Color = Color::Rgb(187, 154, 247);
/// Staged-but-unwritten edits, the active tab, and key hints.
pub(super) const GOLD: Color = Color::Rgb(224, 175, 104);
/// Written, and the value actually in force.
pub(super) const SAGE: Color = Color::Rgb(158, 206, 106);
/// Errors and locked rows.
pub(super) const EMBER: Color = Color::Rgb(247, 118, 142);
/// Primary text.
pub(super) const MIST: Color = Color::Rgb(169, 177, 214);
/// Secondary text and chrome.
pub(super) const SHADE: Color = Color::Rgb(86, 95, 137);
/// Borders and rules.
pub(super) const RULE: Color = Color::Rgb(65, 72, 104);
/// The selected row's ground.
pub(super) const SELECT_BG: Color = Color::Rgb(41, 46, 73);

/// The colour that identifies `scope` everywhere on the screen.
pub(super) fn scope_color(scope: Scope) -> Color {
    match scope {
        Scope::User => WARP,
        Scope::Project => WEFT,
    }
}

/// Body text: values, help prose, the key's own name.
pub(super) fn primary() -> Style {
    Style::default().fg(MIST)
}

/// Chrome: column headers, section words, the half of a line that is context
/// rather than content.
pub(super) fn secondary() -> Style {
    Style::default().fg(SHADE)
}

/// A keyboard shortcut in the footer or a hint.
pub(super) fn key_hint() -> Style {
    Style::default().fg(GOLD).add_modifier(Modifier::BOLD)
}

/// A staged edit: the one colour that means "not written yet".
pub(super) fn staged() -> Style {
    Style::default().fg(GOLD)
}

/// A written value, and the `◆` marking the tier in force.
pub(super) fn written() -> Style {
    Style::default().fg(SAGE)
}

/// A failure, and a row the active scope cannot edit.
pub(super) fn error() -> Style {
    Style::default().fg(EMBER)
}

/// Borders, rules, and the separators between footer pairs.
pub(super) fn rule() -> Style {
    Style::default().fg(RULE)
}

/// The ground under the selected row and the active scope tab.
pub(super) fn selected() -> Style {
    Style::default().bg(SELECT_BG)
}

/// A span in `color` and nothing else — the marker glyphs and tier words that
/// carry their meaning by colour alone.
pub(super) fn plain(color: Color) -> Style {
    Style::default().fg(color)
}

/// A span in `color` with weight, for the labels colour alone is not enough to
/// find: the logo, the active tab, the inspector's title.
pub(super) fn emphasis(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// Every bordered block on this screen: rounded and ruled, so the chrome never
/// competes with the values inside it.
pub(super) fn block(title: Line<'static>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule())
        .title(title)
}

/// A block title in the screen's chrome register: lower-case, dim, and spaced
/// off the border on both sides.
pub(super) fn chrome_title(title: &str) -> Line<'static> {
    Line::from(Span::styled(format!(" {title} "), secondary()))
}
