//! Ratatui rendering for the interactive config editor.
//!
//! Rendering reads state without mutating it, so terminal sizing and styling
//! cannot accidentally alter staged values or their save behavior.
//!
//! Nothing here ever hides a fact to fit: the inspector changes FORM at a
//! narrow width rather than disappearing, and a short frame spends its borders
//! before it spends its content. Columns are the only thing that go, and only
//! the two that repeat what the inspector already says.
//!
//! Every width measurement goes through the ledger's
//! [`text`](crate::commands::status::ui::tui::ledger::text) helpers. The logo
//! and the tier glyphs are not one cell per character, and a `chars().count()`
//! anywhere in this module would misalign the screen for exactly the keys
//! whose values are widest.

/// The status block and the key footer.
mod chrome;
/// The logo band and the scope selector.
mod header;
/// The selected key's detail, as a panel or as a strip.
mod inspector;
/// The key table.
mod table;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use super::state::ConfigState;
use crate::commands::status::ui::tui::ledger::text::text_width;

/// Frame height at which the four-line logo earns its space.
const TALL: u16 = 20;
/// Frame height at which the scope selector and the status line can afford
/// their borders. Below it both keep their content and lose their boxes,
/// which is four rows back for the table.
const ROOMY: u16 = 30;
/// Frame width at which the inspector fits beside the table rather than under
/// it.
const WIDE: u16 = 112;
/// The inspector's width when it sits beside the table.
const INSPECTOR_WIDTH: u16 = 40;
/// The inspector's height when it sits under it: two borders around the two
/// condensed content lines.
const INSPECTOR_HEIGHT: u16 = 4;

/// Draw the logo band, the scope selector, the key table, the inspector, and
/// the chrome.
pub(super) fn draw(frame: &mut Frame, state: &ConfigState) {
    let frame_area = frame.area();
    let tall = frame_area.height >= TALL;
    let roomy = frame_area.height >= ROOMY;
    let chrome_height = if roomy { 3 } else { 1 };
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if tall { 5 } else { 1 }),
            Constraint::Length(chrome_height),
            Constraint::Min(6),
            Constraint::Length(chrome_height),
            Constraint::Length(1),
        ])
        .split(frame_area);
    header::render_logo(frame, areas[0], tall);
    header::render_scope(frame, areas[1], state, roomy);
    render_body(frame, areas[2], state);
    chrome::render_status(frame, areas[3], state, roomy);
    chrome::render_footer(frame, areas[4]);
}

/// Put the inspector beside the table when the width allows, under it when it
/// does not. It is never dropped: the help text and the third tier live
/// nowhere else on the screen.
fn render_body(frame: &mut Frame, area: Rect, state: &ConfigState) {
    if area.width >= WIDE {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(INSPECTOR_WIDTH)])
            .split(area);
        table::render(frame, columns[0], state);
        inspector::render_panel(frame, columns[1], state);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(INSPECTOR_HEIGHT)])
        .split(area);
    table::render(frame, rows[0], state);
    inspector::render_strip(frame, rows[1], state);
}

/// Truncate from the LEFT with a leading `…`.
///
/// For a path, the tail is what identifies it — `…/work/config.toml` still
/// names the file, while a head-truncated `/home/someone/very/long/…` does
/// not. Measured in display cells; a wide character that would straddle the
/// boundary is dropped rather than half-emitted.
pub(super) fn tail(text: &str, width: usize) -> String {
    if text_width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let budget = width - 1;
    let mut kept = Vec::new();
    let mut used = 0;
    for character in text.chars().rev() {
        let character_width = text_width(&character.to_string());
        if used + character_width > budget {
            break;
        }
        kept.push(character);
        used += character_width;
    }
    let mut value = String::from("…");
    value.extend(kept.into_iter().rev());
    value
}
